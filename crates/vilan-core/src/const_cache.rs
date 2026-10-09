//! M110 S4 — the CONST CACHE (`incremental-analysis.md` §5, ruled Q5): a
//! `const` site's evaluation, remembered by what it evaluates.
//!
//! The const pass lowers every site against one shared world
//! (`transformer::ConstWorld`) and hands the interpreter a [`ConstSite`]: the
//! world declarations the site reaches (its callee closure, std's included),
//! the host imports and helpers it names, a prelude declaring the module
//! bindings it reads (an already-folded `const` dependency arrives here as its
//! value), and the site's own expression. That program IS the paper's key —
//! the site's content, the content of every function it can call, and std's
//! (lowered with the rest) — so it is hashed, not re-derived from ids: two
//! analyses that lower a site to the same program evaluate it to the same
//! thing, whatever their entity ids. The one input outside the program is the
//! project the site READS (`asset::read`, `read_dir`, `digest`, `bundle`) and
//! the registry it stages into; each such call is recorded with the answer it
//! got ([`ChannelCall`]), and a hit is served only after every recorded call
//! is asked again of this pass's reader and answered the same — which also
//! re-registers, in order, every effect those calls have on the pass (the
//! tracked inputs, the facts, the staged lines). The interpreter is
//! deterministic in its program and its channel answers, so equal answers
//! mean an equal run.
//!
//! A site whose replay disagrees is evaluated afresh, and the calls the replay
//! already made are not made twice: the fresh run is served their answers
//! ([`Recorder`]), the way it would have made them. A failed evaluation is
//! not remembered — it reports, and the program it reports in is not clean
//! enough to have reached the pass twice in a row anyway.
//!
//! Process-wide and bounded, like the base cache's records; a clean analysis
//! (`incremental::clean_analysis`) neither reads nor writes it. The counts
//! ride the census (`const_cache_hits` / `const_cache_misses`) and the
//! `VILAN_COUNTERS` line.

use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use crate::const_eval::EmittedAsset;
use crate::fx::FxHashMap as HashMap;
use crate::interpreter::{self, AssetReader, ConstValue, Failure, Limits};
use crate::transformer::{ConstSite, js};

/// What one site's evaluation produced — the part of
/// `interpreter::ConstOutcome` the const pass keeps.
#[derive(Clone)]
pub(crate) struct SiteRun {
    pub value: ConstValue,
    pub assets: Vec<EmittedAsset>,
    pub scheduled: Vec<String>,
    pub fuel_used: u64,
}

/// One call a site made through the const pass's reader, with the answer it
/// got.
#[derive(Clone, Debug, PartialEq)]
enum ChannelCall {
    Read(String, Result<String, String>),
    Bundle(String, Option<String>, Result<String, String>),
    ReadDir(String, bool, Result<Vec<String>, String>),
    Digest(String, Result<(String, u64), String>),
    Stage(String, String, String),
    Staged(String, Result<Vec<String>, String>),
}

impl ChannelCall {
    /// The same call asked of `reader`, with the answer it gives now.
    fn ask(&self, reader: &dyn AssetReader) -> ChannelCall {
        match self {
            ChannelCall::Read(path, _) => ChannelCall::Read(path.clone(), reader.read(path)),
            ChannelCall::Bundle(path, target, _) => ChannelCall::Bundle(
                path.clone(),
                target.clone(),
                reader.bundle(path, target.as_deref()),
            ),
            ChannelCall::ReadDir(path, recursive, _) => {
                ChannelCall::ReadDir(path.clone(), *recursive, reader.read_dir(path, *recursive))
            }
            ChannelCall::Digest(path, _) => ChannelCall::Digest(path.clone(), reader.digest(path)),
            ChannelCall::Stage(kind, token, line) => {
                reader.stage(kind, token, line);
                self.clone()
            }
            ChannelCall::Staged(kind, _) => ChannelCall::Staged(kind.clone(), reader.staged(kind)),
        }
    }

    /// Whether `other` is the same QUESTION, whatever either was answered.
    fn same_request(&self, other: &ChannelCall) -> bool {
        match (self, other) {
            (ChannelCall::Read(a, _), ChannelCall::Read(b, _))
            | (ChannelCall::Digest(a, _), ChannelCall::Digest(b, _))
            | (ChannelCall::Staged(a, _), ChannelCall::Staged(b, _)) => a == b,
            (ChannelCall::Bundle(a, x, _), ChannelCall::Bundle(b, y, _)) => a == b && x == y,
            (ChannelCall::ReadDir(a, x, _), ChannelCall::ReadDir(b, y, _)) => a == b && x == y,
            (ChannelCall::Stage(..), ChannelCall::Stage(..)) => self == other,
            _ => false,
        }
    }
}

/// The reader a site evaluates through: every call forwarded to the pass's
/// reader and recorded with its answer — except the calls a failed replay
/// already forwarded, which are answered from that replay so their effects on
/// the pass are not registered twice.
struct Recorder<'r> {
    inner: &'r dyn AssetReader,
    answered: Vec<ChannelCall>,
    next: Cell<usize>,
    /// Set once a call departs from `answered`: everything after it is new.
    departed: Cell<bool>,
    calls: RefCell<Vec<ChannelCall>>,
}

impl<'r> Recorder<'r> {
    fn new(inner: &'r dyn AssetReader, answered: Vec<ChannelCall>) -> Self {
        Recorder {
            inner,
            answered,
            next: Cell::new(0),
            departed: Cell::new(false),
            calls: RefCell::new(Vec::new()),
        }
    }

    fn exchange(&self, request: ChannelCall) -> ChannelCall {
        let index = self.next.get();
        self.next.set(index + 1);
        let answer = match self.answered.get(index) {
            Some(prior) if !self.departed.get() && prior.same_request(&request) => prior.clone(),
            _ => {
                self.departed.set(true);
                request.ask(self.inner)
            }
        };
        self.calls.borrow_mut().push(answer.clone());
        answer
    }
}

impl AssetReader for Recorder<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        match self.exchange(ChannelCall::Read(path.to_string(), Ok(String::new()))) {
            ChannelCall::Read(_, answer) => answer,
            _ => unreachable!("an exchange answers the call it was asked"),
        }
    }

    fn bundle(&self, path: &str, target: Option<&str>) -> Result<String, String> {
        match self.exchange(ChannelCall::Bundle(
            path.to_string(),
            target.map(str::to_string),
            Ok(String::new()),
        )) {
            ChannelCall::Bundle(_, _, answer) => answer,
            _ => unreachable!("an exchange answers the call it was asked"),
        }
    }

    fn read_dir(&self, path: &str, recursive: bool) -> Result<Vec<String>, String> {
        match self.exchange(ChannelCall::ReadDir(
            path.to_string(),
            recursive,
            Ok(Vec::new()),
        )) {
            ChannelCall::ReadDir(_, _, answer) => answer,
            _ => unreachable!("an exchange answers the call it was asked"),
        }
    }

    fn digest(&self, path: &str) -> Result<(String, u64), String> {
        match self.exchange(ChannelCall::Digest(
            path.to_string(),
            Ok((String::new(), 0)),
        )) {
            ChannelCall::Digest(_, answer) => answer,
            _ => unreachable!("an exchange answers the call it was asked"),
        }
    }

    fn stage(&self, kind: &str, token: &str, line: &str) {
        self.exchange(ChannelCall::Stage(
            kind.to_string(),
            token.to_string(),
            line.to_string(),
        ));
    }

    fn staged(&self, kind: &str) -> Result<Vec<String>, String> {
        match self.exchange(ChannelCall::Staged(kind.to_string(), Ok(Vec::new()))) {
            ChannelCall::Staged(_, answer) => answer,
            _ => unreachable!("an exchange answers the call it was asked"),
        }
    }
}

/// One remembered evaluation.
struct CachedSite {
    run: SiteRun,
    transcript: Vec<ChannelCall>,
}

/// How many sites the cache holds before it starts over — kolt's client world
/// evaluates a few hundred, std's own included.
const CACHED_SITES: usize = 8192;

static CACHE: OnceLock<Mutex<HashMap<u64, Arc<CachedSite>>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<u64, Arc<CachedSite>>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::default()))
}

fn lookup(key: u64) -> Option<Arc<CachedSite>> {
    cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .cloned()
}

fn store(key: u64, site: CachedSite) {
    let mut cache = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if cache.len() >= CACHED_SITES {
        cache.clear();
    }
    cache.insert(key, Arc::new(site));
}

/// Empties the cache — the test surface (a pin that must observe a cold pass).
#[doc(hidden)]
pub fn clear() {
    cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

/// A `fmt::Write` that feeds a hasher, so a `Debug` rendering is hashed as it
/// is written rather than built as a string first.
struct HashWriter<'h>(&'h mut DefaultHasher);

impl std::fmt::Write for HashWriter<'_> {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.0.write(text.as_bytes());
        Ok(())
    }
}

fn hash_node(node: &js::Node<'_>, hasher: &mut DefaultHasher) {
    let _ = write!(HashWriter(hasher), "{node:?}");
    // A separator no rendering ends with, so two node lists that concatenate
    // to one text still hash apart.
    hasher.write_u8(0xff);
}

/// The key a site is remembered by: everything [`interpreter::eval_const`]
/// reads besides the project — the lowered program (world declarations,
/// imports, helpers, prelude, body), whether a closure result is admitted,
/// the budgets, and this compiler's version (the interpreter's semantics are
/// the binary's). `world_hashes` memoizes the world declarations' hashes by
/// address for one pass, where the declarations are shared by every site and
/// never move.
pub(crate) fn site_key(
    site: &ConstSite<'_>,
    snapshots: bool,
    limits: Limits,
    world_hashes: &mut HashMap<usize, u64>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Valid only for a CLEAN program (`analyzer-pass-map.md`, recommendation
    // 8): the pass runs only when the analysis reported nothing
    // (`const_eval::evaluate`'s early return), and a site lowered from a
    // program that failed is not the same question — the transformer's
    // lookups assume a clean one. Spelled into the key so that a future
    // caller evaluating a broken program cannot meet a clean one's answers.
    "vilan-const-cache: a clean program".hash(&mut hasher);
    env!("CARGO_PKG_VERSION").hash(&mut hasher);
    snapshots.hash(&mut hasher);
    limits.fuel.hash(&mut hasher);
    limits.call_depth.hash(&mut hasher);
    if !crate::incremental::planted(crate::incremental::Plant::ConstKeyWithoutWorld) {
        site.world.len().hash(&mut hasher);
        for declaration in &site.world {
            let address = std::ptr::from_ref::<js::Node<'_>>(declaration) as usize;
            let hash = *world_hashes.entry(address).or_insert_with(|| {
                let mut declaration_hasher = DefaultHasher::new();
                hash_node(declaration, &mut declaration_hasher);
                declaration_hasher.finish()
            });
            hash.hash(&mut hasher);
        }
    }
    site.imports.hash(&mut hasher);
    site.helpers.hash(&mut hasher);
    site.prelude.len().hash(&mut hasher);
    for node in &site.prelude {
        hash_node(node, &mut hasher);
    }
    site.body.len().hash(&mut hasher);
    for node in site.body {
        hash_node(node, &mut hasher);
    }
    hasher.finish()
}

thread_local! {
    static HITS: Cell<u64> = const { Cell::new(0) };
    static MISSES: Cell<u64> = const { Cell::new(0) };
}

/// This thread's const-cache hits and misses since [`reset_counts`] — the
/// const pass's own tally, read by its `VILAN_COUNTERS` line.
pub fn counts() -> (u64, u64) {
    (HITS.with(Cell::get), MISSES.with(Cell::get))
}

pub(crate) fn reset_counts() {
    HITS.with(|hits| hits.set(0));
    MISSES.with(|misses| misses.set(0));
}

/// Evaluates `site` through the cache: a remembered run whose every channel
/// call answers the same again, or a fresh evaluation that is remembered when
/// it succeeds. A clean analysis evaluates afresh and remembers nothing.
pub(crate) fn evaluate(
    site: &ConstSite<'_>,
    limits: Limits,
    snapshots: bool,
    reader: &dyn AssetReader,
    world_hashes: &mut HashMap<usize, u64>,
) -> Result<SiteRun, Failure> {
    if crate::incremental::clean_requested() {
        return interpreter::eval_const(site, limits, snapshots, Some(reader)).map(run_of);
    }
    let key = site_key(site, snapshots, limits, world_hashes);
    let mut forwarded: Vec<ChannelCall> = Vec::new();
    if let Some(cached) = lookup(key) {
        let unvalidated =
            crate::incremental::planted(crate::incremental::Plant::ConstCacheUnvalidated);
        let mut stands = true;
        for call in &cached.transcript {
            let answer = call.ask(reader);
            let same = unvalidated || answer == *call;
            forwarded.push(answer);
            if !same {
                stands = false;
                break;
            }
        }
        if stands {
            HITS.with(|hits| hits.set(hits.get() + 1));
            return Ok(cached.run.clone());
        }
    }
    MISSES.with(|misses| misses.set(misses.get() + 1));
    let recorder = Recorder::new(reader, forwarded);
    let evaluated = interpreter::eval_const(site, limits, snapshots, Some(&recorder));
    let run = evaluated.map(run_of)?;
    store(
        key,
        CachedSite {
            run: run.clone(),
            transcript: recorder.calls.into_inner(),
        },
    );
    Ok(run)
}

pub(crate) fn run_of(outcome: interpreter::ConstOutcome) -> SiteRun {
    SiteRun {
        value: outcome.value,
        assets: outcome.assets,
        scheduled: outcome.scheduled,
        fuel_used: outcome.fuel_used,
    }
}
