//! R11 door B (Order 54, incr-54b; `r11-instantiations.md` §3) — one record per
//! INSTANCE a world's analysis scanned, replayed through the unchanged worklist.
//!
//! `check_resource_generic_instantiations` is whole-program by construction: an
//! instantiation discovered by PROPAGATION reports at the inner call in the
//! callee's body, another module's, so no per-module window or per-item record
//! (M19, M110 S7) can carry it — and on a served kolt keystroke every one of
//! its 34 instances is a std callee scanned again for the same answer (§1.3 of
//! the paper: all std, zero violations). What an instance's verdict reads
//! (§2) is the callee's body, its resource set, the whole-program sets sliced
//! to the callee (all of them the callee's module's own facts), the known
//! instantiation types' one `Drop` question (B469) and WHO reports first. The
//! first four are a function of the stored world while the callee's module is
//! COLD; the fifth is reproduced by re-running the worklist, which is cheap
//! once the scans are records.
//!
//! So an instance is keyed by `(callee, resources)` — the resources by the
//! callee's own parameter POSITION or the trait's `Self` marker, never a
//! `TypeId` (`editor-latency.md` §3.6) — and its record holds the verdicts the
//! emitters already split: the affine scan's violations (anchored in the
//! body), the exactly-once leaks, the `drop` forwards, the container check's
//! body sites, the lazy offenses, and the propagation EDGES (the body calls
//! whose discovery under the instance's resources instantiated something).
//! Every field is an id or a span of the callee's own body. A served analysis
//! runs the worklist as before (seeds, dedup, first-enqueuer-reports, BFS); an
//! instance whose callee is cold and whose record's digest matches has its
//! verdicts EMITTED through the same emitters at the CURRENT reporting call
//! and its edges re-discovered, scanning nothing; everything else is scanned
//! live, the sets built lazily as before.
//!
//! Three validators (§3): the callee's source is cold — minted by the stored
//! world and in M110 S3's cold set (the entry neither dirtied it nor reaches
//! it through an alias; `Program::cold_entity`'s rule, asked of the analyzer's
//! own state since the checks run before the seam is sealed); the world's key
//! (the records live with M19's checks record, under the base-cache key and
//! the prefix's source hashes, so every cold module's text is byte-identical);
//! and the [`R11InstanceRecord::drop_digest`], recomputed from the CURRENT
//! known types: per resource constraint, whether its bound holds a `Drop`
//! anywhere inside — the one bit `type_contains_drop` reads through `known`
//! for that constraint — plus the program's whole `Drop`-nominal set (a world
//! nominal by id, a hot one by name), so a `Drop` impl appearing anywhere
//! moves every digest for one keystroke and the records are refiled. The container check stays per CALL at replay:
//! it substitutes the reporting call's full bindings into the body's typed
//! sites, which are what the record keeps.
//!
//! Filed only by a seeded analysis (the editor's; a cold `vilan check` never
//! reads one back), on a hit and on a miss alike (the miss builds the world
//! the hits inherit), merged per key; the store never creates a checks record.
//! The plant [`crate::incremental::Plant::R11RecordUnguarded`] ignores the
//! digest at lookup, which the edit-replay differential must see.

use std::sync::Arc;

use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::type_::{Type, TypeId};

use super::{
    Analyzer, BaseCacheKey, CHECKED_CACHE, GenericLeak, R11Instance, ResourceMoveViolation,
    SourceId,
};

/// One resource of an instance's key, in the callee's own terms.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub(super) enum R11ResourceKey {
    /// The callee's generic parameter at this position
    /// (`Function::generic_parameter_constraint_ids`).
    Parameter(u16),
    /// The enclosing impl's generic parameter at this position among the
    /// generics its SUBJECT is written in (`impl List<T>`'s `T`, bound by
    /// reconciling the subject against the receiver — the
    /// `method_call_substitution` channel).
    Subject(u16),
    /// The declaring trait's `Self` marker (B463): a trait default's `self`.
    SelfMarker,
}

/// What an instance is: the callee and the resources it is instantiated at.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(super) struct R11InstanceKey {
    pub callee: Id,
    /// Sorted, as the worklist's dedup key is.
    pub resources: Vec<R11ResourceKey>,
}

/// One instance's verdicts, as the emitters split them — every id and span
/// the callee's own body's.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct R11InstanceRecord {
    /// The `Drop` digest the verdicts were derived under
    /// ([`Analyzer::r11_drop_digest`]); a record whose digest differs from the
    /// current one is a key without a record.
    pub drop_digest: u64,
    /// The affine scan's violations (`scan_instantiated_body`).
    pub violations: Vec<ResourceMoveViolation>,
    /// The exactly-once leaks: `(note anchor, binding, kind)`, in emission
    /// order (`generic_leaks`).
    pub leaks: Vec<(Id, Id, GenericLeak)>,
    /// The `drop` calls that would forward a resource (`generic_drop_forwards`).
    pub drop_forwards: Vec<Id>,
    /// The body's typed sites the container check substitutes the reporting
    /// call's bindings into: the parameters and the body's bindings, in
    /// `(span, id)` order (`container_sites`).
    pub container_sites: Vec<Id>,
    /// B344's offenses: `(parameter position, parameter)`; whether the
    /// reporting call already spoke for a position is asked at emission.
    pub lazy_offenses: Vec<(usize, Id)>,
    /// The body calls whose discovery under the instance's resources
    /// instantiated something — re-discovered live at replay (a few type
    /// questions per call) so the dedup and the reporting order stay the
    /// worklist's.
    pub edges: Vec<Id>,
}

impl R11InstanceRecord {
    pub(super) fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.violations.len() * std::mem::size_of::<ResourceMoveViolation>()
            + self.leaks.len() * std::mem::size_of::<(Id, Id, GenericLeak)>()
            + (self.drop_forwards.len() + self.container_sites.len() + self.edges.len())
                * std::mem::size_of::<Id>()
            + self.lazy_offenses.len() * std::mem::size_of::<(usize, Id)>()
    }
}

/// One world's instance records (`WorldChecks::r11`).
#[derive(Debug, Default)]
pub(crate) struct R11Records {
    instances: HashMap<R11InstanceKey, R11InstanceRecord>,
    pub bytes: usize,
}

impl R11Records {
    fn insert(&mut self, key: R11InstanceKey, record: R11InstanceRecord) {
        let now = record.bytes() + std::mem::size_of::<R11InstanceKey>();
        if let Some(displaced) = self.instances.insert(key, record) {
            self.bytes = self
                .bytes
                .saturating_sub(displaced.bytes() + std::mem::size_of::<R11InstanceKey>());
        }
        self.bytes += now;
    }
}

/// The instance records for `key`, if the checks record still describes this
/// world's content.
pub(super) fn checked_cache_lookup_r11(
    key: &BaseCacheKey,
    source_hashes: &[u64],
) -> Option<Arc<R11Records>> {
    let cache = CHECKED_CACHE.get()?;
    let mut state = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let recorded = state.records.get(key)?;
    if recorded.source_hashes.len() != source_hashes.len()
        || recorded.source_hashes[1..] != source_hashes[1..]
    {
        return None;
    }
    let served = recorded.r11.clone()?;
    let tick = state.next_tick();
    if let Some(recorded) = state.records.get_mut(key) {
        recorded.last_used = tick;
    }
    Some(served)
}

/// Files this analysis's instance records beside the checks record the
/// diagnostics store wrote for this key — never creating one; a key already
/// recorded is replaced by the newer derivation.
pub(super) fn checked_cache_store_r11(
    key: &BaseCacheKey,
    source_hashes: &[u64],
    derived: Vec<(R11InstanceKey, R11InstanceRecord)>,
) {
    if derived.is_empty() {
        return;
    }
    let Some(cache) = CHECKED_CACHE.get() else {
        return;
    };
    let mut state = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let previous_bytes = state.records.get(key).map_or(0, |record| record.bytes);
    let Some(recorded) = state.records.get_mut(key) else {
        return;
    };
    if recorded.source_hashes.len() != source_hashes.len()
        || recorded.source_hashes[1..] != source_hashes[1..]
    {
        return;
    }
    let mut records = recorded
        .r11
        .take()
        .map(|records| {
            Arc::try_unwrap(records).unwrap_or_else(|shared| R11Records {
                instances: shared.instances.clone(),
                bytes: shared.bytes,
            })
        })
        .unwrap_or_default();
    recorded.bytes = recorded.bytes.saturating_sub(records.bytes);
    for (instance_key, record) in derived {
        records.insert(instance_key, record);
    }
    recorded.bytes += records.bytes;
    recorded.r11 = Some(Arc::new(records));
    let bytes = recorded.bytes;
    let tick = state.next_tick();
    if let Some(recorded) = state.records.get_mut(key) {
        recorded.last_used = tick;
    }
    state.bytes = state
        .bytes
        .saturating_sub(previous_bytes)
        .saturating_add(bytes);
    state.evict_to_budget(Some(key));
}

/// What one analysis's replay reads: the served records and the `Drop` facts
/// every digest is computed against, built once at the first instance.
pub(super) struct R11ReplayContext {
    pub served: Option<Arc<R11Records>>,
    /// Whether this analysis FILES the instances it scans (a seeded analysis
    /// with a key to file under).
    pub filing: bool,
    /// The `Drop`-implementing nominals (`drop_implementing_nominals`).
    pub drop_nominals: HashSet<Id>,
    /// That set, hashed ([`Analyzer::r11_drop_set_digest`]).
    pub drop_set_digest: u64,
}

impl<'src> Analyzer<'src> {
    /// The replay context, or `None` when this analysis neither serves nor
    /// files instance records (no stored world, nothing seeded).
    pub(super) fn r11_replay_context(&mut self) -> Option<R11ReplayContext> {
        let served = self.r11_records_served.take();
        let filing = self.r11_records_filing;
        if served.is_none() && !filing {
            return None;
        }
        self.prefix_entity_end?;
        let drop_nominals = self.drop_implementing_nominals();
        let drop_set_digest = self.r11_drop_set_digest(&drop_nominals);
        Some(R11ReplayContext {
            served,
            filing,
            drop_nominals,
            drop_set_digest,
        })
    }

    /// Whether `callee` is cold for this analysis: minted by the stored world,
    /// in a source the entry neither dirtied nor reaches through an alias
    /// (M110 S3's cold set, `Program::cold_entity`'s rule) — the condition
    /// under which its body, its slices and its propagation are the stored
    /// world's and a record of them may be served or filed.
    fn r11_callee_is_cold(&self, callee: Id) -> bool {
        let Some(boundary) = self.prefix_entity_end else {
            return false;
        };
        if callee.0 >= boundary {
            return false;
        }
        self.source_of_id(callee)
            .is_some_and(|source| self.r11_cold_sources.binary_search(&source).is_ok())
    }

    /// An instance's key in the callee's own terms, or `None` for a resource
    /// the callee does not declare (nothing to key by).
    pub(super) fn r11_instance_key(&self, instance: &R11Instance) -> Option<R11InstanceKey> {
        let constraints = self
            .functions
            .get(&instance.callee)
            .map(|function| function.generic_parameter_constraint_ids.as_slice())
            .unwrap_or_default();
        let mut subject_generics: Option<Vec<TypeId>> = None;
        let mut resources = Vec::with_capacity(instance.resources.len());
        for resource in &instance.resources {
            let key = if matches!(
                self.borrow_type_by_type_id(*resource),
                Type::Trait(_, arguments) if arguments.is_empty()
            ) {
                R11ResourceKey::SelfMarker
            } else if let Some(position) = constraints
                .iter()
                .position(|constraint| constraint == resource)
            {
                R11ResourceKey::Parameter(u16::try_from(position).ok()?)
            } else {
                let generics = match &subject_generics {
                    Some(generics) => generics,
                    None => {
                        subject_generics.insert(self.r11_impl_subject_generics(instance.callee)?)
                    }
                };
                let position = generics.iter().position(|generic| generic == resource)?;
                R11ResourceKey::Subject(u16::try_from(position).ok()?)
            };
            resources.push(key);
        }
        resources.sort_unstable();
        Some(R11InstanceKey {
            callee: instance.callee,
            resources,
        })
    }

    /// The generic parameters the subject of `callee`'s enclosing impl is
    /// written in, in first-seen order — the terms an impl method's
    /// substitution binds (`r11_call_type_bindings`'s second channel).
    fn r11_impl_subject_generics(&self, callee: Id) -> Option<Vec<TypeId>> {
        let declares = |member: &Id| {
            *member == callee
                || matches!(
                    self.expr_id_to_expr_map.get(member),
                    Some(super::Expr::Function(function_id)) if *function_id == callee
                )
        };
        let implementation = self
            .implementations
            .iter()
            .find(|implementation| implementation.declarations.values().any(declares))?;
        Some(self.generic_parameters_of(implementation.subject))
    }

    /// The digest an instance's `Drop` question (B469) is a function of: per
    /// resource, whether what it is bound to at THIS reporting site holds a
    /// `Drop` anywhere inside — `type_contains_drop` reads the known types
    /// through `known` exactly there, so two instances agreeing on every bit
    /// (and on the program's `Drop` set) admit the same partial moves —
    /// folded with [`Self::r11_drop_set_digest`].
    pub(super) fn r11_drop_digest(
        &mut self,
        instance: &R11Instance,
        context: &R11ReplayContext,
    ) -> u64 {
        let known = self.r11_instantiation_types(instance);
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let fold = |value: u64, hash: &mut u64| {
            *hash ^= value;
            *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        };
        for (position, resource) in instance.resources.iter().enumerate() {
            let bit: u64 = match known.get(resource).copied() {
                Some(bound) => u64::from(self.type_contains_drop(
                    bound,
                    &known,
                    &context.drop_nominals,
                    &mut HashSet::default(),
                )),
                // Unknown: the question answers "may drop" for it, on every
                // analysis alike.
                None => 2,
            };
            fold(position as u64, &mut hash);
            fold(bit, &mut hash);
        }
        fold(context.drop_set_digest, &mut hash);
        hash
    }

    /// The program's `Drop`-implementing nominal set, hashed: a stored-world
    /// nominal by its id (the same on every hit, §2.1), one minted past the
    /// world — the entry's, a hot module's — by its name and file (a hot id
    /// may renumber between keystrokes; a name does not). The whole set,
    /// not the half the known types reach: a `Drop` impl written in a hot
    /// module for a COLD type is a change to a nominal a cold body may name
    /// outright, which no walk from the known types would meet — the hole
    /// `drop_nominals_fingerprints` closes the same way for the drop scan's
    /// enrolment. The price is one live pass when a `Drop` impl appears or
    /// goes (every record is refiled under the new digest on that keystroke);
    /// every other keystroke serves.
    fn r11_drop_set_digest(&self, drop_nominals: &HashSet<Id>) -> u64 {
        let Some(boundary) = self.prefix_entity_end else {
            return 0;
        };
        let mut world: Vec<u32> = Vec::new();
        let mut hot: Vec<(u32, &'src str)> = Vec::new();
        for id in drop_nominals {
            if id.0 < boundary {
                world.push(id.0);
                continue;
            }
            let name = match self.structs.get(id) {
                Some(struct_) => struct_.name,
                None => match self.enums.get(id) {
                    Some(enum_) => enum_.name,
                    None => continue,
                },
            };
            hot.push((
                self.source_of_id(*id).map_or(u32::MAX, |source| source.0),
                name,
            ));
        }
        world.sort_unstable();
        hot.sort_unstable();
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut fold = |byte: u8| {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        };
        for id in world {
            id.to_le_bytes().into_iter().for_each(&mut fold);
        }
        fold(0xff);
        for (source, name) in hot {
            source.to_le_bytes().into_iter().for_each(&mut fold);
            name.bytes().for_each(&mut fold);
            fold(0);
        }
        hash
    }

    /// The served record for `instance`, when its callee is cold, its key is
    /// recorded and the record's digest is the current one (or the plant
    /// waives the digest).
    pub(super) fn r11_served_record(
        &mut self,
        instance: &R11Instance,
        context: &R11ReplayContext,
    ) -> Option<R11InstanceRecord> {
        let served = context.served.as_ref()?;
        if !self.r11_callee_is_cold(instance.callee) {
            return None;
        }
        let key = self.r11_instance_key(instance)?;
        let record = served.instances.get(&key)?;
        let unguarded = crate::incremental::planted(crate::incremental::Plant::R11RecordUnguarded);
        if !unguarded && record.drop_digest != self.r11_drop_digest(instance, context) {
            return None;
        }
        Some(record.clone())
    }

    /// Files a live scan's verdicts for the instance, when this analysis
    /// files at all and the callee is cold (a hot callee's ids are this
    /// analysis's own; a dirtied module's slots are this keystroke's).
    pub(super) fn r11_file_record(
        &mut self,
        instance: &R11Instance,
        context: &R11ReplayContext,
        mut record: R11InstanceRecord,
    ) {
        if !context.filing || !self.r11_callee_is_cold(instance.callee) {
            return;
        }
        let Some(key) = self.r11_instance_key(instance) else {
            return;
        };
        record.drop_digest = self.r11_drop_digest(instance, context);
        self.r11_records_derived.push((key, record));
    }
}

/// The cold sources of an analysis for M110 S3's seam and R11's records:
/// every stored-world source below `prefix_len` the entry neither dirtied
/// nor reaches through an alias of itself. Sorted.
pub(super) fn cold_sources(
    entry_dirty_sources: &HashSet<SourceId>,
    alias_reaching: &HashSet<SourceId>,
    prefix_len: usize,
) -> Vec<SourceId> {
    (1..prefix_len as u32)
        .map(SourceId)
        .filter(|source| !entry_dirty_sources.contains(source) && !alias_reaching.contains(source))
        .collect()
}
