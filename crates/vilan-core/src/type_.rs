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
    // A tuple: its element types, and the LABELS its positions carry (B569,
    // `named-tuple-fields.md`) — `(x: f64, y: f64)`. The labels are names for
    // positions, never part of the type's identity: equality and hashing
    // ignore them ([`TupleLabels`]), unification carries them, mono and the
    // emitters erase them.
    Tuple(Vec<TypeId>, TupleLabels),
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

/// The labels of a tuple type's positions (B569, `named-tuple-fields.md`
/// §10): `None` for an unlabelled tuple, one label per slot otherwise — a
/// tuple labels every slot or none.
///
/// **Carried, never compared.** `PartialEq` holds for any two values and
/// `Hash` writes nothing, so `(x: f64, y: f64)` and `(f64, f64)` are one
/// [`Type`] to every map, memo and comparison that keys on it — the
/// instantiation keys mono builds, the selection memos, the conformance
/// tables. That is what erases labels at monomorphization: two label sets
/// can never split an instance. Code that must SEE the labels — the
/// printer, member access, the literal's by-name match, the contradiction
/// refusal — reads them through the accessors here; [`TupleLabels::spelled_alike`]
/// is the comparison that does look, for the one place that interns a type
/// by its printed form.
#[derive(Clone, Debug, Default)]
pub struct TupleLabels(Option<std::sync::Arc<[Box<str>]>>);

impl TupleLabels {
    /// An unlabelled tuple's labels.
    pub const NONE: TupleLabels = TupleLabels(None);

    /// One label per slot, in slot order.
    pub fn new<S: Into<Box<str>>>(labels: impl IntoIterator<Item = S>) -> TupleLabels {
        TupleLabels(Some(labels.into_iter().map(Into::into).collect()))
    }

    /// The labels, when the tuple has them.
    pub fn labels(&self) -> Option<&[Box<str>]> {
        self.0.as_deref()
    }

    pub fn is_labelled(&self) -> bool {
        self.0.is_some()
    }

    /// The label of slot `index`.
    pub fn get(&self, index: usize) -> Option<&str> {
        self.0.as_deref()?.get(index).map(|label| &**label)
    }

    /// The slot `label` names.
    pub fn position(&self, label: &str) -> Option<usize> {
        self.0.as_deref()?.iter().position(|each| &**each == label)
    }

    /// These labels for a tuple of `arity` slots: kept when they label
    /// exactly that many, dropped otherwise (a label set describes ONE
    /// arity, and a slot list rebuilt at another has nothing to name).
    pub fn for_arity(&self, arity: usize) -> TupleLabels {
        match self.labels() {
            Some(labels) if labels.len() == arity => self.clone(),
            _ => TupleLabels::NONE,
        }
    }

    /// The first label both carry at DIFFERENT positions (B569 §4.3): the
    /// one pair of label sets that does not reconcile. The same set
    /// reordered is the common case (`(x, y)` into `(y, x)`), and one
    /// renamed slot is the same mistake (`(x, y)` into `(y, z)` moves `y`).
    /// Disjoint sets reconcile by position, and shared labels at the same
    /// positions agree.
    pub fn contradiction<'a>(&'a self, other: &TupleLabels) -> Option<&'a str> {
        let (mine, theirs) = (self.labels()?, other.labels()?);
        mine.iter().enumerate().find_map(|(index, label)| {
            let position = theirs.iter().position(|each| each == label)?;
            (position != index).then_some(&**label)
        })
    }

    /// The labels a reconcile keeps: `self`'s when it has them, else
    /// `other`'s.
    pub fn or(&self, other: &TupleLabels) -> TupleLabels {
        if self.is_labelled() {
            self.clone()
        } else {
            other.clone()
        }
    }

    /// Whether the two print alike — the comparison [`PartialEq`] declines.
    pub fn spelled_alike(&self, other: &TupleLabels) -> bool {
        self.labels() == other.labels()
    }
}

impl PartialEq for TupleLabels {
    fn eq(&self, _: &TupleLabels) -> bool {
        true
    }
}

impl Eq for TupleLabels {}

impl std::hash::Hash for TupleLabels {
    fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

/// The program's type slots, indexed by [`TypeId`].
///
/// Ids are minted densely and in order (`Analyzer::new_type_id` is a counter,
/// and `type_id_sources` is already indexed by it), so the table is a `Vec`
/// and a read is an index. It was a hash map keyed by that same counter: a
/// hash per read, a control byte and a 7/8 load factor per slot, and a
/// doubling at a power-of-two threshold — which is where kolt's cold check
/// found 20 MB of its peak RSS when Order 49's std growth took its server
/// leg's slot count from 221k to 230k, across 7/8 of 2^18 (the map rehashed
/// into 2^19 buckets of 88 bytes, 46 MB, with the old table live beside the
/// new one while it did). The `Vec` holds the same slots in 23 MB, and its
/// own doubling waits for 2^18 slots.
///
/// The API is the subset of the map's the compiler used — `get`, `insert`,
/// `len`, `keys`, `iter` — with the same semantics (`insert` past the end
/// fills the gap, so an id minted before its slot is written reads `None`
/// until it is), so that a reader keeps its shape; `keys` and `iter` hand
/// the id by value, since no id is stored.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TypeTable {
    slots: Vec<Option<Type>>,
    filled: usize,
}

impl TypeTable {
    /// The type in slot `type_id`, if one was written.
    #[inline]
    pub fn get(&self, type_id: &TypeId) -> Option<&Type> {
        self.slots.get(type_id.0 as usize).and_then(Option::as_ref)
    }

    /// Whether slot `type_id` holds a type.
    pub fn contains_key(&self, type_id: &TypeId) -> bool {
        self.get(type_id).is_some()
    }

    /// Writes slot `type_id`, handing back what it held.
    pub fn insert(&mut self, type_id: TypeId, type_: Type) -> Option<Type> {
        let index = type_id.0 as usize;
        if index >= self.slots.len() {
            self.slots.resize_with(index + 1, || None);
        }
        let previous = self.slots[index].replace(type_);
        if previous.is_none() {
            self.filled += 1;
        }
        previous
    }

    /// How many slots hold a type.
    pub fn len(&self) -> usize {
        self.filled
    }

    /// Whether no slot holds a type.
    pub fn is_empty(&self) -> bool {
        self.filled == 0
    }

    /// The ids of the slots that hold a type, in id order.
    pub fn keys(&self) -> impl Iterator<Item = TypeId> + '_ {
        self.iter().map(|(type_id, _)| type_id)
    }

    /// Every written slot with its id, in id order.
    pub fn iter(&self) -> impl Iterator<Item = (TypeId, &Type)> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.as_ref().map(|type_| (TypeId(index as u32), type_)))
    }
}

impl std::fmt::Debug for TypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "TypeId({})", self.0)
    }
}

pub type SubstitutionContext = HashMap<TypeId, TypeId>;

#[cfg(test)]
mod type_table_tests {
    use super::{Type, TypeId, TypeTable};
    use crate::id::Id;

    #[test]
    fn a_slot_reads_back_what_was_written_and_nothing_before_it_is_written() {
        let mut table = TypeTable::default();
        assert!(table.is_empty());
        assert_eq!(table.get(&TypeId(0)), None);
        assert_eq!(table.insert(TypeId(0), Type::Any), None);
        assert_eq!(table.insert(TypeId(1), Type::Never), None);
        assert_eq!(table.get(&TypeId(0)), Some(&Type::Any));
        assert_eq!(table.get(&TypeId(1)), Some(&Type::Never));
        assert_eq!(table.get(&TypeId(2)), None);
        assert!(!table.contains_key(&TypeId(2)));
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn a_rewrite_hands_back_the_previous_type_and_keeps_the_count() {
        let mut table = TypeTable::default();
        table.insert(TypeId(0), Type::Unknown);
        assert_eq!(
            table.insert(TypeId(0), Type::Struct(Id(7), vec![])),
            Some(Type::Unknown)
        );
        assert_eq!(table.get(&TypeId(0)), Some(&Type::Struct(Id(7), vec![])));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn a_write_past_the_end_fills_the_gap_with_empty_slots() {
        let mut table = TypeTable::default();
        table.insert(TypeId(3), Type::Void);
        assert_eq!(table.len(), 1);
        assert_eq!(table.get(&TypeId(0)), None);
        assert_eq!(table.get(&TypeId(2)), None);
        assert_eq!(table.get(&TypeId(3)), Some(&Type::Void));
        assert_eq!(table.keys().collect::<Vec<_>>(), vec![TypeId(3)]);
        // The gap is written later, out of id order: `iter` still walks by id.
        table.insert(TypeId(1), Type::Any);
        assert_eq!(
            table.iter().collect::<Vec<_>>(),
            vec![(TypeId(1), &Type::Any), (TypeId(3), &Type::Void)]
        );
        assert_eq!(table.len(), 2);
    }
}
