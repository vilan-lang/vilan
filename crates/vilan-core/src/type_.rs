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
    "str", "i32", "u32", "f64", "BigInt", "null", "i8", "u8", "i16", "u16", "i53", "u53", "f32",
];

/// The numeric-literal type suffixes the analyzer accepts (`42u32`, `1.5f`,
/// `0n`); any other suffix is a hard error (numeric-types.md §3 — `5i64` names
/// the rename to `i53`). One source of truth: the book's highlight.js theme
/// (`vilan/docs/theme/vilan.js`) spells this list inside its number regex, and
/// `crates/vilan-cli/tests/grammar_sync.rs` holds it to this one — the D15 audit
/// found the theme current and the TextMate grammar a release behind on the
/// sibling primitive-type list, which is the drift that gate closes.
pub const NUMERIC_SUFFIXES: &[&str] = &[
    "i8", "u8", "i16", "u16", "i32", "u32", "i53", "u53", "f", "f32", "f64", "n",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Any,
    // The type of expressions that never produce a value: `panic(..)`,
    // `ret ..`, `jump break`/`continue`. Never unifies by YIELDING to the
    // other side (a diverging match leg doesn't constrain the match's
    // type), unlike `Any`, which absorbs. Internal — not written in source.
    Never,
    Closure(Vec<TypeId>, TypeId),
    // A nominal enum/struct and its type arguments (`Option<i32>` ->
    // `Enum(option_id, [i32])`, `List<str>` -> `Struct(list_id, [str])`). The
    // arguments are empty for a non-generic type, or where they are not (yet)
    // known; member/variant resolution substitutes the type's declared
    // parameters with them.
    Enum(Id, Vec<TypeId>),
    Function(Id),
    Generic(TypeId),
    Module(Id),
    Struct(Id, Vec<TypeId>),
    // A trait and its generic arguments (`Display` -> `Trait(display_id, [])`,
    // `Into<bool>` -> `Trait(into_id, [bool])`, `Readable<U>` ->
    // `Trait(readable_id, [U])`). The arguments drive parameterized-trait impl
    // selection and a mapped trait template's inversion.
    Trait(Id, Vec<TypeId>),
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

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

impl std::fmt::Debug for TypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "TypeId({})", self.0)
    }
}

pub type SubstitutionContext = HashMap<TypeId, TypeId>;
