use crate::fx::FxHashMap as HashMap;
use crate::id::Id;

/// The scalar primitive type names — each backed by a JS value (a number or a
/// string), so a `&mut` view of one lowers to a `(base, key)` pair rather than
/// an aggregate reference, and assigning one never aliases. `bool` is a scalar
/// too but is a numeric *enum*, not a struct, so it is handled alongside this
/// list (never in it) at each view-pointee check — the analyzer's
/// `is_scalar_view_pointee` and the transformer's `resolves_to_scalar_view_pointee`.
/// One source of truth: those two classifiers drifted once (the transformer
/// carried its own copy of the names and never grew the `bool` case), which
/// miscompiled a generic `&mut T` resolving to `bool`.
pub const SCALAR_PRIMITIVE_NAMES: &[&str] = &[
    "str", "i32", "u32", "f64", "BigInt", "null", "i8", "u8", "i16", "u16", "i53", "u53", "usize",
    "f32",
];

/// The numeric PRIMITIVE type names — the scalar primitives minus the three
/// that are not numbers. The emission verdicts an arithmetic expression carries
/// (truncating division, unsigned bitwise) are a property of one of these and
/// of nothing else, so B370's context record is filtered by this list; beside
/// `SCALAR_PRIMITIVE_NAMES` so the two cannot drift apart unnoticed.
pub const NUMERIC_PRIMITIVE_NAMES: &[&str] = &[
    "i8", "u8", "i16", "u16", "i32", "u32", "i53", "u53", "usize", "f32", "f64", "BigInt",
];

/// The numeric-literal type suffixes the analyzer accepts (`42u32`, `1.5f`,
/// `0n`); any other suffix is a hard error (numeric-types.md §3 — `5i64` names
/// the rename to `i53`). One source of truth: the book's highlight.js theme
/// (`vilan/docs/theme/vilan.js`) spells this list inside its number regex, and
/// `crates/vilan-cli/tests/grammar_sync.rs` holds it to this one — the D15 audit
/// found the theme current and the TextMate grammar a release behind on the
/// sibling primitive-type list, which is the drift that gate closes.
pub const NUMERIC_SUFFIXES: &[&str] = &[
    "i8", "u8", "i16", "u16", "i32", "u32", "i53", "u53", "usize", "f", "f32", "f64", "n",
];

// `Hash` because the resolved type is a memo KEY: impl selection over a
// `TypeId` is a pure function of the `Type` that id resolves to (the id's
// identity never enters the walk — see `dispatch_refine::refined_edges`), and
// a program mints one id per expression, so keying a selection memo on the id
// would memoize nothing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    Any,
    // The type of expressions that never produce a value: `panic(..)`,
    // `ret ..`, `jump break`/`continue`. Never unifies by YIELDING to the
    // other side (a diverging match leg doesn't constrain the match's
    // type), unlike `Any`, which absorbs. Internal — not written in source.
    Never,
    // A closure type: the parameter types, the return type, and the `context`
    // clause it carries (B309) — the context bindings an INJECTED closure is
    // threaded with, in written order, empty for an ordinary closure.
    //
    // The clause is part of the TYPE, not a side-band keyed by the parameter
    // that happened to declare it (`ambient-owner.md` §5 v1 recorded it by
    // parameter id, which is why it flowed through no field, generic argument
    // or return). Carrying it here is what lets `body: (|| View) context
    // owner_scope` be a struct FIELD, a generic ARGUMENT and a RETURN type, and
    // what lets the coverage check follow the value wherever it flows.
    //
    // UNIFICATION IGNORES IT. Compatibility is the parameters and the return:
    // a closure LITERAL is born clause-less and takes the clause of the
    // position it lands in (that is the whole point — the literal defers its
    // context binding to its call sites instead of capturing at creation), so
    // demanding equal clauses here would refuse every legal program. The
    // discipline that keeps a clause-carrying value honest is
    // `context::thread_contexts`' value-flow rule — a call, a forward to a
    // same-clause position, or `run` — and it is closed by default: a use the
    // rule does not name is refused, not threaded.
    //
    // The fourth slot is each parameter's MODE (B495, `closure-type-views.md`):
    // see [`ParameterMode`]. Empty means the modes are UNSTATED — a type the
    // analyzer synthesized as an expectation (`|T| U` for an element walk),
    // which constrains no mode and passes every parameter by value.
    Closure(Vec<TypeId>, TypeId, Vec<Id>, Vec<ParameterMode>),
    // A nominal enum/struct and its type arguments (`Option<i32>` ->
    // `Enum(option_id, [i32])`, `List<str>` -> `Struct(list_id, [str])`). The
    // arguments are empty for a non-generic type, or where they are not (yet)
    // known; member/variant resolution substitutes the type's declared
    // parameters with them.
    Enum(Id, Vec<TypeId>),
    Function(Id),
    // A mention of a generic parameter, by the CONSTRAINT's type id — the id
    // whose own `Type` is the parameter's bound (`Trait(Greet, [])` for
    // `<type T: Greet>`, `Any` for an unbounded one).
    //
    // **`Generic(constraint)` is the ONE spelling of a parameter in a nominal
    // declaration's body** (B366): a struct field, an enum variant's payload,
    // a nested nominal's or tuple's argument, and an impl SUBJECT's argument
    // all carry it, for user, std, `external`, bounded, multi-parameter,
    // recursive and `[derive]`-generated declarations alike. The bare
    // constraint id is never a body type. That is what lets one walk bind a
    // declaration's parameters —
    // [`crate::impl_select::bind_subject`](crate::impl_select::bind_subject)
    // is that walk, and it matches this node and nothing else — and the
    // emitters may rely on it. `tests/nominal_generic_spelling.rs` is the gate.
    Generic(TypeId),
    Module(Id),
    Struct(Id, Vec<TypeId>),
    // A trait and its generic arguments (`Display` -> `Trait(display_id, [])`,
    // `Into<bool>` -> `Trait(into_id, [bool])`, `Readable<U>` ->
    // `Trait(readable_id, [U])`). The arguments drive parameterized-trait impl
    // selection and a mapped trait template's inversion.
    Trait(Id, Vec<TypeId>),
    // B4/A124 R3: a TRAIT OBJECT — the erased pair `(value, vtable)` over a
    // trait whose members are all dispatchable. `dyn Source<i32>` ->
    // `Dyn(source_id, [i32])`, carrying exactly the arguments `Trait` carries.
    //
    // It is a VALUE type and `Trait` is not: that is the whole distinction
    // §0 of trait-objects.md says the one representation was missing. Every
    // reader that asks "is this a value" answers yes here and no there.
    Dyn(Id, Vec<TypeId>),
    Tuple(Vec<TypeId>),
    // A fixed-length array `[T; n]` — the element type and a compile-time-known
    // length (`[i32; 4]` -> `Array(i32, 4)`). Unlike `List<T>` (a growable
    // `Struct(list_id, [T])`), the length is part of the type, so `[i32; 3]` and
    // `[i32; 4]` are distinct and neither resizes. Lowers to a plain JS array.
    Array(TypeId, usize),
    // A mapped tuple type `(U in T: F<U>)`, symbolic while the source tuple `T` is
    // still abstract: the binder `U`'s generic id, the source tuple type, and the
    // template `F<U>`. Expands to a concrete `Tuple` once `T` resolves to one
    // (each element `X` maps to `F[U := X]`).
    Mapped(TypeId, TypeId, TypeId),
    Unknown,
    Unresolved,
    Void,
}

/// How a closure type passes one parameter (B495, `closure-type-views.md`
/// §3 (b)): by value, as a view (`&T`), or as a writable view (`&mut T`).
///
/// A view "is tracked beside the type, never in it" everywhere else — a view
/// is not a generic argument, a field or an element — and a closure type's
/// parameters are the one place it is written INTO a type, because they are
/// positions, not values. So the mode is a property of the closure type's
/// parameter slot and of nothing a type variable can stand for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Value,
    View,
    MutView,
}

impl Mode {
    /// The `&`/`&mut ` a view parameter prints before its type.
    pub fn prefix(self) -> &'static str {
        match self {
            Mode::Value => "",
            Mode::View => "&",
            Mode::MutView => "&mut ",
        }
    }

    /// `Some(mutable)` for a view, `None` for a value — the shape the
    /// emitters' per-parameter view tables use.
    pub fn view(self) -> Option<bool> {
        match self {
            Mode::Value => None,
            Mode::View => Some(false),
            Mode::MutView => Some(true),
        }
    }
}

/// One parameter slot of a [`Type::Closure`]'s modes (B495).
///
/// A closure TYPE that was written (`|&str| void`), a literal's parameter
/// that spells its type (`|c: &str|`, `|c: str|`), and a named function's
/// declared parameter all state their mode: [`ParameterMode::Written`]. A
/// literal's parameter written BARE (`|c|`) states none: its mode is the one
/// of the written position the literal lands in, and it takes that mode where
/// the two types meet — inside unification, the way a literal takes the
/// position's `context` clause (B309) — which is why the slot names the
/// parameter that adopts.
///
/// Modes take part in `Type`'s equality and hashing: `|str| void` and
/// `|&str| void` are different calling conventions, so they are different
/// types (Q4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParameterMode {
    Written(Mode),
    /// A closure literal's bare parameter, by the parameter's id.
    Open(Id),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

impl std::fmt::Debug for TypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "TypeId({})", self.0)
    }
}

pub type SubstitutionContext = HashMap<TypeId, TypeId>;
