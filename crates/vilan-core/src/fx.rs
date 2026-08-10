//! A multiply-xor-rotate hasher for the analyzer's id-keyed tables
//! (`proposal/suite-speed.md` §10, backlog E48).
//!
//! **Why.** E47's profile of a cold `vilan check` left hashing as the largest
//! single family in the remaining cost: SipHash — `d_rounds`, `c_rounds`,
//! `Hasher::write`, `u8to64_le` — was **13.2%** of `rpc_server`'s 10.2e9 Ir.
//! The analyzer keys ~500 tables on [`Id`](crate::id::Id),
//! [`TypeId`](crate::type_::TypeId) and
//! [`SourceId`](crate::analyzer::SourceId), every one of them a `u32` newtype.
//! SipHash-1-3 is a good hash for four bytes of key and a bad *deal* for it:
//! its per-call setup — load two 64-bit keys, buffer the input, run the
//! finalization rounds — costs the same whether it is fed 4 bytes or 4 KiB.
//! Against a `u32` key that setup **is** the work.
//!
//! **Provenance, stated honestly.** This is an independent implementation of
//! the hash *idea* published as FxHash — one multiply by an odd constant, one
//! rotate, one xor per word — which originated in Firefox's `HashMap` and
//! reached Rust through `rustc-hash`, the hasher rustc itself runs on.
//! `rustc-hash` is dual Apache-2.0/MIT and its algorithm is public domain in
//! substance (it is four lines of arithmetic). **No code was vendored** — this
//! crate takes zero new dependencies (E48's first constraint), so the family is
//! reimplemented here, and the two decisions below are this file's own, argued
//! from vilan's key distribution rather than inherited.
//!
//! **Decision one: the mixing constant.** `SEED` is the 64-bit odd constant the
//! FxHash family has used since Firefox — near 2^64/φ, the multiplier
//! Fibonacci hashing calls for. Any odd constant with a well-mixed bit pattern
//! works; this one is published, well-exercised, and not a magic number of this
//! author's invention.
//!
//! **Decision two: no finalizer, on purpose, and here is what it trades.**
//! `rustc-hash` 2.x ends with a rotate, because multiplication carries entropy
//! *upward* and `hashbrown` indexes buckets with the hash's **low** bits (the
//! top seven become the control-byte tag). That rotate is right for its inputs
//! and wrong for ours. Multiplication by an odd constant is a **bijection
//! modulo any power of two** — so for a table of 2^n buckets, dense sequential
//! ids `0, 1, 2, …` land one per bucket with *zero* collisions, which is better
//! than any general-purpose hash can do. A finalizing rotate gives that away
//! and buys protection against *strided* keys instead. Both halves, measured
//! over 4096 keys in 4096 buckets by the two tests below:
//!
//! | keys                  | no finalizer  | `finish` = `rotate_left(20)` |
//! |-----------------------|---------------|------------------------------|
//! | dense `0, 1, 2, …`    | **4096/4096** | 3931/4096                    |
//! | strided `0, 8, 16, …` | 512/4096      | **3889/4096**                |
//!
//! vilan is squarely the dense case and cannot be the strided one: `Id` comes
//! from `self.entity_id += 1` and `TypeId` from `self.type_id += 1`, both plain
//! counters with no block allocation and no alignment, and a table that holds
//! only *some* of those ids holds an arbitrary subset of a dense range, which
//! the bijection scatters exactly as a random hash would. So the finalizer-free
//! form is the right one here — a small, real win, not a large one.
//! `sequential_ids_spread_across_every_bucket` pins it and goes red (3931) under
//! a planted finalizing rotate; the strided row is recorded by its twin as the
//! known price rather than asserted to be good.
//!
//! **Where it is applied.** The id-keyed modules: `analyzer`, `async_infer`,
//! `context`, `const_eval`, `init_order`, `call_graph`, `chunks`, `type_`,
//! `platform_color`, `transformer`, `macros`. The string-keyed modules —
//! `bindgen`, `manifest`, `interpreter` — keep `std`'s default hasher, since
//! they are not in the cold-analysis hot path and have nothing to win.
//!
//! **The collision-resistance question, answered rather than waved at.** This
//! hasher is fast, not adversarial-proof: an attacker who can choose keys can
//! find collisions and drive a table quadratic. That matters when untrusted
//! input reaches a hash table *on a machine the attacker does not own*, and
//! vilan has no such surface. The CLI compiles files its operator chose; the
//! LSP compiles the editor's own workspace; the playground's analyzer is the
//! `wasm32` build running client-side in the visitor's own tab, so a crafted
//! program's only victim is its author. The string-keyed tables that do exist
//! inside the converted modules (module names, member names — 21 of the
//! analyzer's 382) ride along, and the same argument covers them. Should vilan
//! ever analyze untrusted source in a shared process, this decision is the one
//! to revisit, and this paragraph is why.
//!
//! **Determinism.** Unlike `std`'s `RandomState`, which reseeds every map, this
//! hasher is seeded with a constant — so iteration order becomes a stable
//! function of the keys. That is *more* deterministic than what it replaces,
//! but it costs a guard rail: E38/E44 made diagnostics and emission
//! hash-order-independent, and a randomly seeded map is what made an
//! order-dependence regression *observable*. [`enable_seed_shuffle`] gives that
//! instrument back — under it, every table is seeded from a process counter,
//! exactly as `RandomState` behaved. `diagnostic_determinism` turns it on, and
//! `VILAN_HASH_SHUFFLE=1` turns it on for a whole suite run.

use std::hash::{BuildHasher, Hasher};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// The odd multiplier, ~2^64/φ: the FxHash family's published constant.
const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

/// How far the accumulator turns before each word is folded in. Five bits per
/// word is what keeps a multi-word key's earlier words from being ground away
/// by the multiplies that follow.
const ROTATE: u32 = 5;

/// A `HashMap` keyed by small integers — ids, type ids, source ids.
pub type FxHashMap<K, V> = std::collections::HashMap<K, V, FxBuildHasher>;

/// A `HashSet` of small integers — the twin of [`FxHashMap`].
pub type FxHashSet<T> = std::collections::HashSet<T, FxBuildHasher>;

/// The hash-order shuffle, forced on in-process by [`enable_seed_shuffle`].
static FORCED_SHUFFLE: AtomicBool = AtomicBool::new(false);

/// `VILAN_HASH_SHUFFLE`, read once. Set it to anything but `0` to run a whole
/// suite with per-table seeds, which is the old `RandomState` net.
static ENV_SHUFFLE: OnceLock<bool> = OnceLock::new();

/// Hands out one distinct seed per table while shuffling is on.
static SEED_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Seed every table built from here on from a process counter, so iteration
/// order varies table to table exactly as `std`'s `RandomState` made it vary.
///
/// This is the instrument a determinism pin needs: with the constant seed, an
/// order-dependent answer is *stably wrong* and repetition cannot see it. Call
/// it before the analyses being compared, and only in a test.
pub fn enable_seed_shuffle() {
    FORCED_SHUFFLE.store(true, Ordering::Relaxed);
}

/// Whether new tables get a per-table seed.
#[inline]
fn shuffling() -> bool {
    FORCED_SHUFFLE.load(Ordering::Relaxed)
        || *ENV_SHUFFLE
            .get_or_init(|| std::env::var("VILAN_HASH_SHUFFLE").is_ok_and(|value| value != "0"))
}

/// Builds [`FxHasher`]s for one table.
///
/// `Default` is the only constructor a `HashMap` uses, which is why the shuffle
/// decision lives there: `FxHashMap::default()` is the seam every table goes
/// through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FxBuildHasher {
    seed: u64,
}

impl Default for FxBuildHasher {
    #[inline]
    fn default() -> Self {
        if shuffling() {
            // Fibonacci-mix the counter: consecutive counts must produce seeds
            // that differ in every bit position, or "shuffled" tables would
            // still agree on iteration order and the instrument would lie.
            let count = SEED_COUNTER.fetch_add(1, Ordering::Relaxed);
            Self {
                seed: count.wrapping_add(1).wrapping_mul(SEED).rotate_left(32),
            }
        } else {
            Self { seed: 0 }
        }
    }
}

impl BuildHasher for FxBuildHasher {
    type Hasher = FxHasher;

    #[inline]
    fn build_hasher(&self) -> FxHasher {
        FxHasher { hash: self.seed }
    }
}

/// One multiply-xor-rotate pass per word of the key.
#[derive(Debug, Clone, Copy, Default)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    /// The whole hash, in one line: turn the accumulator, fold the word in,
    /// spread it with one multiply.
    #[inline]
    fn add_word(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(ROTATE) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    // `Hasher`'s default `write_uN` forwards to `write(&n.to_ne_bytes())` — the
    // byte-slice path, with its length bookkeeping and tail branches. Overriding
    // each one is the point of this file: a `u32` key becomes a rotate, an xor
    // and a multiply, with nothing buffered.
    #[inline]
    fn write_u8(&mut self, n: u8) {
        self.add_word(u64::from(n));
    }

    #[inline]
    fn write_u16(&mut self, n: u16) {
        self.add_word(u64::from(n));
    }

    #[inline]
    fn write_u32(&mut self, n: u32) {
        self.add_word(u64::from(n));
    }

    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.add_word(n);
    }

    #[inline]
    fn write_u128(&mut self, n: u128) {
        self.add_word(n as u64);
        self.add_word((n >> 64) as u64);
    }

    #[inline]
    fn write_usize(&mut self, n: usize) {
        self.add_word(n as u64);
    }

    // Little-endian rather than native, so a hash — and therefore a table's
    // iteration order — is the same number on every target vilan builds for
    // (`wasm32` included). Costs a byte-swap on big-endian hardware and nothing
    // anywhere vilan actually runs.
    fn write(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while let Some((word, tail)) = rest.split_first_chunk::<8>() {
            self.add_word(u64::from_le_bytes(*word));
            rest = tail;
        }
        if let Some((half, tail)) = rest.split_first_chunk::<4>() {
            self.add_word(u64::from(u32::from_le_bytes(*half)));
            rest = tail;
        }
        if let Some((half, tail)) = rest.split_first_chunk::<2>() {
            self.add_word(u64::from(u16::from_le_bytes(*half)));
            rest = tail;
        }
        if let Some(&byte) = rest.first() {
            self.add_word(u64::from(byte));
        }
    }

    #[inline]
    fn finish(&self) -> u64 {
        // No finalizer: see the module header. `hashbrown` takes the bucket
        // index from the low bits, and the low bits of a multiply by an odd
        // constant are already a bijection of the key's low bits — which is
        // exactly the distribution dense ids want.
        self.hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::Hash;

    /// The unseeded builder, spelled out rather than taken from `Default`.
    ///
    /// Every test below pins the ALGORITHM, which is the constant-seed
    /// behaviour, and `Default` stops producing it under `VILAN_HASH_SHUFFLE=1`
    /// — a mode the module header tells people to run whole suites in. Naming
    /// the seed here is what keeps `VILAN_HASH_SHUFFLE=1 cargo nextest run
    /// --workspace` green, so that instruction stays true.
    fn unseeded() -> FxBuildHasher {
        FxBuildHasher { seed: 0 }
    }

    fn hash_of<T: Hash>(value: &T) -> u64 {
        unseeded().hash_one(value)
    }

    /// The claim the missing finalizer rests on: for a power-of-two bucket
    /// count, dense sequential ids land one per bucket with no collisions at
    /// all. Goes red under a planted finalizing `rotate_left(20)`, which reaches
    /// 3931 of the 4096 — an ordinary hash's coverage, where this is perfect.
    #[test]
    fn sequential_ids_spread_across_every_bucket() {
        const BUCKETS: u64 = 4096;
        let mut seen = vec![false; BUCKETS as usize];
        for id in 0u32..BUCKETS as u32 {
            seen[(hash_of(&crate::id::Id(id)) % BUCKETS) as usize] = true;
        }
        assert_eq!(
            seen.iter().filter(|reached| **reached).count(),
            BUCKETS as usize,
            "dense ids must reach every bucket; a finalizing rotate is the way to lose this"
        );
    }

    /// Strided ids — every eighth — are the case a finalizer exists to protect,
    /// and the one this hasher trades away knowingly: 512 buckets eight deep,
    /// where the planted `rotate_left(20)` reaches 3889. Recorded as a
    /// measurement rather than asserted to be good. It stays acceptable only
    /// because vilan's id allocators are counters and cannot produce a stride;
    /// if one ever does, this test is where that shows.
    #[test]
    fn strided_ids_still_reach_a_useful_share_of_the_buckets() {
        const BUCKETS: u64 = 4096;
        let mut seen = vec![false; BUCKETS as usize];
        for step in 0u32..BUCKETS as u32 {
            seen[(hash_of(&crate::id::Id(step * 8)) % BUCKETS) as usize] = true;
        }
        let reached = seen.iter().filter(|reached| **reached).count();
        assert_eq!(
            reached,
            BUCKETS as usize / 8,
            "a stride of eight collapses onto an eighth of the buckets, eight deep — \
             known, bounded, and not what vilan's counters produce"
        );
    }

    /// Distinct ids must not agree on a hash over any range the analyzer could
    /// plausibly reach.
    #[test]
    fn a_hundred_thousand_ids_have_a_hundred_thousand_hashes() {
        let hashes: std::collections::HashSet<u64> = (0u32..100_000)
            .map(|id| hash_of(&crate::type_::TypeId(id)))
            .collect();
        assert_eq!(hashes.len(), 100_000);
    }

    /// The unseeded hasher is a pure function of the key: two builders, two
    /// processes, two architectures, one number.
    #[test]
    fn the_default_seed_makes_hashing_a_pure_function() {
        assert_eq!(hash_of(&crate::id::Id(7)), hash_of(&crate::id::Id(7)));
        assert_eq!(hash_of(&crate::id::Id(7)), 7u64.wrapping_mul(SEED));
        assert_eq!(hash_of(&"vilan"), hash_of(&"vilan"));
    }

    /// Multi-byte keys go through the word loop and its tail, and every length
    /// must reach a distinct hash — a tail branch that dropped its bytes would
    /// still pass a round-trip test.
    #[test]
    fn every_prefix_length_hashes_differently() {
        let text = "the quick brown fox jumps over the lazy dog, twice over";
        let hashes: std::collections::HashSet<u64> = (0..=text.len())
            .map(|length| {
                let mut hasher = FxHasher::default();
                hasher.write(&text.as_bytes()[..length]);
                hasher.finish()
            })
            .collect();
        assert_eq!(hashes.len(), text.len() + 1);
    }

    /// A shuffled builder is a different hash function, which is the whole
    /// point of the escape hatch — and shuffling must be OFF unless asked for,
    /// or every measurement in `suite-speed.md` §10 would be noise.
    ///
    /// Both directions are pinned, because `VILAN_HASH_SHUFFLE=1` is a
    /// supported way to run the suite and "on" has to be as true as "off".
    #[test]
    fn the_shuffle_reseeds_and_is_off_until_asked_for() {
        if shuffling() {
            assert_ne!(
                FxBuildHasher::default(),
                FxBuildHasher::default(),
                "asked for: every table gets its own seed"
            );
        } else {
            assert_eq!(
                FxBuildHasher::default(),
                FxBuildHasher::default(),
                "off by default: every table gets the constant seed"
            );
        }

        // Not `enable_seed_shuffle()`: that is process-wide and one-way, and
        // this test shares its process with the ones above.
        let first = FxBuildHasher {
            seed: 1u64.wrapping_mul(SEED).rotate_left(32),
        };
        let second = FxBuildHasher {
            seed: 2u64.wrapping_mul(SEED).rotate_left(32),
        };
        assert_ne!(first, second);
        assert_ne!(
            first.hash_one(crate::id::Id(3)),
            second.hash_one(crate::id::Id(3)),
            "consecutive seeds must give different hash functions"
        );
        assert_ne!(first.hash_one(crate::id::Id(3)), hash_of(&crate::id::Id(3)));
    }

    /// The aliases behave as maps and sets, not merely as types that compile.
    #[test]
    fn the_aliases_are_working_tables() {
        let mut map: FxHashMap<crate::id::Id, u32> = FxHashMap::default();
        for id in 0u32..1000 {
            map.insert(crate::id::Id(id), id * 2);
        }
        assert_eq!(map.len(), 1000);
        assert_eq!(map.get(&crate::id::Id(999)), Some(&1998));
        assert_eq!(map.remove(&crate::id::Id(0)), Some(0));
        assert_eq!(map.get(&crate::id::Id(0)), None);

        let set: FxHashSet<crate::type_::TypeId> = (0u32..1000).map(crate::type_::TypeId).collect();
        assert_eq!(set.len(), 1000);
        assert!(set.contains(&crate::type_::TypeId(500)));
        assert!(!set.contains(&crate::type_::TypeId(1000)));
    }
}
