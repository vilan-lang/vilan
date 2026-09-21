//! B366: a nominal declaration's generic parameter has ONE spelling in its own
//! body — `Type::Generic(constraint_id)` — and this is the gate that says so.
//!
//! The question is an emitter's. Both backends have to bind a declaration's
//! parameters from a concrete type: `Leaf(T)` against `Leaf(i32)` gives
//! `{T -> i32}`, and `impl List<type T>` against `List<str>` gives
//! `{T -> str}`. `impl_select::bind_subject` is that walk, and it matches
//! `Type::Generic` and nothing else. The emit-Rust backend wrote a second copy
//! that ALSO accepted a bare constraint id as a body type, on the reading that
//! "a generic enum's payload records the constraint id itself, whose `Type` is
//! `Any`" — a second spelling nobody could point at, and a branch no program
//! reaches.
//!
//! It does not exist, and the honest way to retire a defence is to pin the
//! invariant it was defending against rather than to delete the branch and
//! hope. So: every type reachable from a struct's fields, an enum variant's
//! payloads and an impl subject's arguments — through nested nominals, tuples
//! and arrays — is walked, and a declared constraint id appearing as one of
//! them is a failure that names where it was found.
//!
//! The reach matters more than the count, so the fixture declares every shape
//! that could plausibly diverge: an unbounded parameter, a bounded one, two
//! parameters, a payload that is a nested nominal, a tuple, an `Option`, a
//! RECURSIVE enum carrying itself, and a `[derive]`-generated pair whose items
//! the macro engine writes rather than the author. std rides in as well — its
//! `List`, `Option`, `Result` and `Shared` are `external` declarations, which
//! is the other way a declaration can reach the tables.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use vilan_core::type_::{Type, TypeId};
use vilan_core::{PackageSpec, Platform, Workspace, analyze_source};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// Every generic shape a nominal declaration can take, and one use of each so
/// nothing is pruned before the tables are read.
const FIXTURE: &str = r#"
    import std::io::print;

    trait Greet { fun greet(self): str; }

    struct Plain<type T> { value: T, many: List<T>, pair: (T, T), opt: Option<T> }
    struct Bounded<type T: Greet> { value: T, many: List<T> }
    struct Two<type A, type B> { left: A, right: B, both: (A, B) }

    enum PlainE<type T> { Leaf(T), Many(List<T>), Pair((T, T)), Opt(Option<T>), Empty }
    enum BoundedE<type T: Greet> { One(T), Nested(PlainE<T>) }
    enum Recursive<type T> { Node(T, List<Recursive<T>>), Nil }

    [derive(Debug)]
    struct Derived<type T> { value: T }

    [derive(Debug)]
    enum DerivedE<type T> { Only(T) }

    impl i32 with Greet { fun greet(self): str { "i" } }

    fun main() {
        let plain = Plain { value = 1, many = [1], pair = (1, 1), opt = Some(1) };
        let bounded = Bounded { value = 1, many = [1] };
        let two = Two { left = 1, right = "s", both = (1, "s") };
        let many = PlainE::Many([2]);
        let one = BoundedE::One(3);
        let node = Recursive::Node(4, []);
        let derived = Derived { value = 5 };
        let only = DerivedE::Only(6);
        print(plain.value + bounded.value + two.left + derived.value);
        match many { PlainE::Many(let v) => print(v.len()), _ => print(0), }
        match one { BoundedE::One(let v) => print(v), _ => print(0), }
        match node { Recursive::Node(let v, _) => print(v), _ => print(0), }
        match only { DerivedE::Only(let v) => print(v), }
    }
    "#;

/// What one walk of the world found: how many `Generic(..)` nodes sit in
/// declaration bodies, and every bare constraint id that does (with where).
struct Census {
    parameters: usize,
    generic_nodes: usize,
    bare: Vec<String>,
}

/// The walk itself, over a type map rather than over a `Program`, so the
/// DETECTOR can be exercised on an input the analyzer will not produce — which
/// is the only way to prove it non-vacuous. Planting the second spelling in the
/// analyzer does not work: a body type that is a bare constraint id is not a
/// key of the type map at all, so the analysis panics on the first lookup long
/// before any gate sees it (`borrow_type_by_type_id`'s unwrap). That is a
/// stronger statement of the invariant than this gate makes, and a worse
/// failure to read, which is why the gate stays.
fn walk<S: std::hash::BuildHasher>(
    types: &HashMap<TypeId, Type, S>,
    parameters: &HashSet<TypeId>,
    mut pending: Vec<(String, TypeId)>,
) -> Census {
    let mut generic_nodes = 0usize;
    let mut bare: Vec<String> = Vec::new();
    let mut seen: HashSet<TypeId> = HashSet::new();
    while let Some((where_, type_id)) = pending.pop() {
        if parameters.contains(&type_id) {
            bare.push(format!(
                "{where_} is the bare constraint id {type_id:?} (type {:?}) rather \
                 than `Generic({type_id:?})`",
                types.get(&type_id)
            ));
            continue;
        }
        // The type graph is a graph: a recursive enum reaches itself.
        if !seen.insert(type_id) {
            continue;
        }
        match types.get(&type_id) {
            Some(Type::Generic(_)) => generic_nodes += 1,
            Some(
                Type::Struct(_, arguments) | Type::Enum(_, arguments) | Type::Tuple(arguments),
            ) => {
                for argument in arguments.clone() {
                    pending.push((where_.clone(), argument));
                }
            }
            Some(Type::Array(element, _)) => {
                let element = *element;
                pending.push((where_, element));
            }
            _ => {}
        }
    }
    bare.sort();
    Census {
        parameters: 0,
        generic_nodes,
        bare,
    }
}

fn census() -> Census {
    let spec = std_spec();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let (program, errors) = analyze_source(
                FIXTURE,
                &spec,
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let program = program.expect("the fixture analyzes");
            assert!(
                errors.is_empty(),
                "the fixture must be clean: {:?}",
                errors
                    .into_iter()
                    .map(|error| error.msg)
                    .collect::<Vec<_>>()
            );

            // Every parameter DECLARED in the world, std included.
            let mut parameters: HashSet<TypeId> = HashSet::new();
            for struct_ in program.structs.values() {
                parameters.extend(struct_.generic_parameter_constraint_ids.iter().copied());
            }
            for enum_ in program.enums.values() {
                parameters.extend(enum_.generic_parameter_constraint_ids.iter().copied());
            }

            // Every type a declaration's body reaches, with where it came from
            // so a failure can be read rather than hunted.
            let mut pending: Vec<(String, TypeId)> = Vec::new();
            for struct_ in program.structs.values() {
                for field in &struct_.fields {
                    pending.push((
                        format!("field `{}` of struct `{}`", field.name, struct_.name),
                        field.type_id,
                    ));
                }
            }
            for enum_ in program.enums.values() {
                for variant in &enum_.variants {
                    for payload in &variant.data_type_ids {
                        pending.push((
                            format!("a payload of `{}::{}`", enum_.name, variant.name),
                            *payload,
                        ));
                    }
                }
            }
            for implementation in &program.implementations {
                pending.push((
                    "an impl subject's argument".to_string(),
                    implementation.subject,
                ));
            }

            let mut census = walk(&program.type_id_to_type_map, &parameters, pending);
            census.parameters = parameters.len();
            census
        })
        .expect("spawn the analysis worker")
        .join()
        .expect("the analysis worker finished")
}

#[test]
fn a_nominal_parameter_is_spelled_generic_in_its_declarations_body_and_nowhere_bare() {
    let census = census();

    // The floors keep the gate from passing because it looked at nothing. They
    // are well under what the world carries (44 parameters and 131 `Generic`
    // nodes at the sha this landed), so std growing or shrinking does not touch
    // them — only the mechanism breaking does.
    assert!(
        census.parameters >= 20,
        "the walk found only {} declared generic parameters, so it is not \
         reading the world's declarations",
        census.parameters
    );
    assert!(
        census.generic_nodes >= 40,
        "the walk found only {} `Generic(..)` nodes in declaration bodies, so \
         it is not reading their types",
        census.generic_nodes
    );

    assert!(
        census.bare.is_empty(),
        "{} declaration body type(s) spell a generic parameter as the bare \
         constraint id. That is a SECOND spelling, and every walk that binds a \
         declaration's parameters — `impl_select::bind_subject`, and the \
         emit-Rust backend's own — matches `Type::Generic` alone, so a bare one \
         binds nothing and the parameter is emitted unsubstituted. Mint \
         `Generic(constraint)` at the site that recorded this instead of \
         teaching the walks a second shape (B366):\n  {}",
        census.bare.len(),
        census.bare.join("\n  ")
    );
}

/// The detector itself, on inputs the analyzer will not produce — which is how
/// this gate is shown to be non-vacuous. It CANNOT be shown the usual way, by
/// planting the defect: rewriting the enum walk to record a payload as its bare
/// constraint id makes the analysis PANIC (`borrow_type_by_type_id`'s unwrap,
/// on a type id the map has no entry for) before any gate runs. The second
/// spelling is not merely unused — vilan-core cannot carry it.
#[test]
fn the_spelling_walk_finds_a_bare_constraint_id_and_passes_a_wrapped_one() {
    let constraint = TypeId(1);
    let wrapped = TypeId(2);
    let nominal = TypeId(3);
    let tuple = TypeId(4);
    let element = TypeId(5);
    let array = TypeId(6);

    let mut types: HashMap<TypeId, Type> = HashMap::new();
    types.insert(constraint, Type::Any);
    types.insert(wrapped, Type::Generic(constraint));
    types.insert(nominal, Type::Struct(vilan_core::id::Id(7), vec![wrapped]));
    types.insert(tuple, Type::Tuple(vec![wrapped, nominal]));
    types.insert(element, Type::Generic(constraint));
    types.insert(array, Type::Array(element, 3));
    let parameters: HashSet<TypeId> = HashSet::from([constraint]);

    // The good spelling, through every container the walk descends: four
    // `Generic(..)` nodes found and nothing reported.
    let good = walk(
        &types,
        &parameters,
        vec![
            ("a field".to_string(), wrapped),
            ("a payload".to_string(), nominal),
            ("a tuple payload".to_string(), tuple),
            ("an array field".to_string(), array),
        ],
    );
    assert!(
        good.bare.is_empty(),
        "the wrapped spelling must not be reported: {:?}",
        good.bare
    );
    assert_eq!(
        good.generic_nodes, 2,
        "each distinct `Generic(..)` type id counts once, however many roots \
         reach it"
    );

    // The bad spelling at a root, and nested one level down — both found, and
    // each named by the place it was reached from.
    let mut with_bare = types.clone();
    with_bare.insert(
        nominal,
        Type::Struct(vilan_core::id::Id(7), vec![constraint]),
    );
    let bad = walk(
        &with_bare,
        &parameters,
        vec![
            ("a field".to_string(), constraint),
            ("a payload".to_string(), nominal),
        ],
    );
    assert_eq!(
        bad.bare.len(),
        2,
        "both bare spellings must be found: {:?}",
        bad.bare
    );
    assert!(
        bad.bare.iter().any(|found| found.contains("a field"))
            && bad.bare.iter().any(|found| found.contains("a payload")),
        "each finding must name where it was reached from: {:?}",
        bad.bare
    );
}
