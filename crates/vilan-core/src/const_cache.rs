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
//!
//! # The disk layer (M135)
//!
//! The key is the program, not an entity id, and a run is plain data - so the
//! table can outlive the process. A front end that owns a package's cache
//! directory ([`set_disk_root`]: the CLI, beside the macro-expansion table in
//! `<dir>/.cache/`) gets `const-sites` there: every site this process evaluated,
//! with its transcript of project reads, loaded once at the first site and
//! rewritten whole (a temporary and a rename) when the pass added to it. A hit
//! from disk is a hit like any other: the transcript is asked again of this
//! pass's reader before the run is served. The header names the format, the
//! toolchain version and the analyzer's BUILD ([`crate::BUILD_ID`], G28): the
//! interpreter's semantics are the binary's, and two dev builds share a version.
//! A table that is not exactly what this build wrote is a miss, never an error,
//! and a site whose result is not plain data (a `const let` closure snapshot
//! names an entity id) is simply not written.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
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
    disk_remember();
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
    disk_load_once();
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

// --- The disk layer (M135) ---

const DISK_FORMAT: &str = "vilan-const-cache 1";
/// The most sites one table keeps (the in-memory bound's twin): past it the
/// smallest keys stay, so the file is a function of its contents and not of the
/// order sites were met in.
const DISK_ENTRY_CAP: usize = CACHED_SITES;

/// The disk layer's state: where the table lives, whether it was read, and whether
/// this process added a site the file does not hold. The sites themselves live in the
/// in-memory cache only - the file is rendered from it at the flush - so the layer
/// holds no second copy of a table that kolt's client makes 2.3 MB.
struct Disk {
    path: Option<PathBuf>,
    loaded: bool,
    dirty: bool,
}

static DISK: OnceLock<Mutex<Disk>> = OnceLock::new();

fn disk() -> std::sync::MutexGuard<'static, Disk> {
    DISK.get_or_init(|| {
        Mutex::new(Disk {
            path: None,
            loaded: false,
            dirty: false,
        })
    })
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The header this build writes and reads back.
fn disk_stamp() -> String {
    format!(
        "{DISK_FORMAT}\ntoolchain {}\nbuild {}",
        env!("CARGO_PKG_VERSION"),
        crate::BUILD_ID
    )
}

/// Points the cache at a package's cache directory (`<dir>/.cache/const-sites`),
/// or off (`None`, the default: the language server and the playground keep the
/// in-memory cache they always had and write nothing). Changing the directory
/// forgets what was loaded for the old one.
pub fn set_disk_root(root: Option<&Path>) {
    let path = root.map(|root| {
        crate::util::canonical_path_of_unwritten(root.join(".cache").join("const-sites"))
    });
    let mut disk = disk();
    if disk.path != path {
        *disk = Disk {
            path,
            loaded: false,
            dirty: false,
        };
    }
}

/// Reads the table once per process (per directory): every entry that decodes
/// is also put in the in-memory cache, where a lookup finds it like any other.
fn disk_load_once() {
    let path = {
        let mut disk = disk();
        if disk.loaded {
            return;
        }
        let Some(path) = disk.path.clone() else {
            return;
        };
        disk.loaded = true;
        path
    };
    let Some(entries) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| parse_disk_table(&text, &disk_stamp()))
    else {
        return;
    };
    let mut memory = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (key, site) in entries {
        if memory.len() < CACHED_SITES {
            memory.entry(key).or_insert_with(|| Arc::new(site));
        }
    }
}

/// Notes that the next flush has something to write. A site whose result cannot
/// leave the process (a closure snapshot) is skipped at the flush, not here.
fn disk_remember() {
    let mut disk = disk();
    if disk.path.is_some() {
        disk.dirty = true;
    }
}

/// Rewrites the table if this process added to it: whole, through a temporary
/// and a rename, so a reader never sees half a table. Every IO failure means "no
/// cache", never "no build". Called at the end of the const pass.
pub(crate) fn flush_disk() {
    let path = {
        let mut disk = disk();
        if !disk.dirty {
            return;
        }
        let Some(path) = disk.path.clone() else {
            return;
        };
        disk.dirty = false;
        path
    };
    // The table is the persistable part of the in-memory cache, in key order: what a
    // previous process wrote is in it (loaded at the first site) next to what this one
    // evaluated, and an entry the cache evicted is simply not carried forward.
    let mut text = disk_stamp();
    text.push('\n');
    {
        let memory = cache()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ordered: BTreeMap<u64, &Arc<CachedSite>> =
            memory.iter().map(|(k, v)| (*k, v)).collect();
        for (key, site) in ordered.into_iter().take(DISK_ENTRY_CAP) {
            let Some(payload) = encode_site(site) else {
                continue;
            };
            let _ = writeln!(text, "{key:016x} {}", payload.len());
            text.push_str(&payload);
            text.push('\n');
        }
    }
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    // Per write, not merely per process: a workspace's legs analyze on threads
    // of their own and may flush one package's table at once.
    static FLUSH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let temporary = parent.join(format!(
        "const-sites.{}.{}.tmp",
        std::process::id(),
        FLUSH.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    if std::fs::write(&temporary, text).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return;
    }
    if std::fs::rename(&temporary, &path).is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
}

/// Forgets the disk layer's state, so the next evaluation re-reads the file: the
/// test surface that makes a second PROCESS reachable inside one (with [`clear`]).
#[doc(hidden)]
pub fn forget_disk() {
    let mut disk = disk();
    disk.loaded = false;
    disk.dirty = false;
}

type DecodedTable = Vec<(u64, CachedSite)>;

/// A written table, or `None` for anything that is not exactly what this build
/// wrote - a stale stamp, a truncated entry, a payload that does not decode.
fn parse_disk_table(text: &str, stamp: &str) -> Option<DecodedTable> {
    let mut rest = text.strip_prefix(stamp)?.strip_prefix('\n')?;
    let mut entries = Vec::new();
    while !rest.is_empty() {
        let (header, after) = rest.split_once('\n')?;
        let (key, length) = header.split_once(' ')?;
        let key = u64::from_str_radix(key, 16).ok()?;
        let length: usize = length.parse().ok()?;
        let payload = after.get(..length)?;
        if after.as_bytes().get(length) != Some(&b'\n') {
            return None;
        }
        let site = decode_site(payload)?;
        entries.push((key, site));
        rest = &after[length + 1..];
    }
    Some(entries)
}

// The payload grammar. Text, framed by structure: a number ends at `;`, a string
// is `<byte length>:<bytes>`, a list is `<count>;` then its items. Floats travel
// as their bits, so a `NaN` payload and `-0` survive unchanged.

fn put_string(out: &mut String, value: &str) {
    let _ = write!(out, "{}:{value}", value.len());
}

fn put_number(out: &mut String, value: u64) {
    let _ = write!(out, "{value};");
}

fn put_strings(out: &mut String, values: &[String]) {
    put_number(out, values.len() as u64);
    for value in values {
        put_string(out, value);
    }
}

fn put_result<T>(
    out: &mut String,
    result: &Result<T, String>,
    put_ok: impl FnOnce(&mut String, &T),
) {
    match result {
        Ok(value) => {
            out.push('o');
            put_ok(out, value);
        }
        Err(message) => {
            out.push('x');
            put_string(out, message);
        }
    }
}

fn encode_value(out: &mut String, value: &ConstValue) -> Option<()> {
    match value {
        ConstValue::Undefined => out.push('u'),
        ConstValue::Null => out.push('z'),
        ConstValue::Bool(true) => out.push('t'),
        ConstValue::Bool(false) => out.push('f'),
        ConstValue::Number(number) => {
            let _ = write!(out, "n{:016x};", number.to_bits());
        }
        ConstValue::BigInt(number) => {
            let _ = write!(out, "b{number};");
        }
        ConstValue::Str(text) => {
            out.push('s');
            put_string(out, text);
        }
        ConstValue::Array(items) | ConstValue::Set(items) => {
            out.push(if matches!(value, ConstValue::Array(_)) {
                'a'
            } else {
                'e'
            });
            put_number(out, items.len() as u64);
            for item in items {
                encode_value(out, item)?;
            }
        }
        ConstValue::Map(entries) => {
            out.push('m');
            put_number(out, entries.len() as u64);
            for (key, item) in entries {
                encode_value(out, key)?;
                encode_value(out, item)?;
            }
        }
        // A closure snapshot names an ANALYZER entity (`body: Id`), which means
        // nothing to another process: the one result that stays in memory.
        ConstValue::Callable { .. } => return None,
    }
    Some(())
}

fn encode_call(out: &mut String, call: &ChannelCall) {
    match call {
        ChannelCall::Read(path, answer) => {
            out.push('R');
            put_string(out, path);
            put_result(out, answer, |out, text| put_string(out, text));
        }
        ChannelCall::Bundle(path, target, answer) => {
            out.push('B');
            put_string(out, path);
            match target {
                Some(target) => {
                    out.push('+');
                    put_string(out, target);
                }
                None => out.push('-'),
            }
            put_result(out, answer, |out, text| put_string(out, text));
        }
        ChannelCall::ReadDir(path, recursive, answer) => {
            out.push('D');
            put_string(out, path);
            out.push(if *recursive { 't' } else { 'f' });
            put_result(out, answer, |out, names| put_strings(out, names));
        }
        ChannelCall::Digest(path, answer) => {
            out.push('G');
            put_string(out, path);
            put_result(out, answer, |out, (digest, length)| {
                put_string(out, digest);
                put_number(out, *length);
            });
        }
        ChannelCall::Stage(kind, token, line) => {
            out.push('S');
            put_string(out, kind);
            put_string(out, token);
            put_string(out, line);
        }
        ChannelCall::Staged(kind, answer) => {
            out.push('T');
            put_string(out, kind);
            put_result(out, answer, |out, lines| put_strings(out, lines));
        }
    }
}

/// A site's payload, or `None` when its result cannot leave the process.
fn encode_site(site: &CachedSite) -> Option<String> {
    let mut out = String::new();
    encode_value(&mut out, &site.run.value)?;
    put_number(&mut out, site.run.assets.len() as u64);
    for asset in &site.run.assets {
        put_string(&mut out, &asset.kind);
        put_string(&mut out, &asset.key);
        put_string(&mut out, &asset.line);
    }
    put_strings(&mut out, &site.run.scheduled);
    put_number(&mut out, site.run.fuel_used);
    put_number(&mut out, site.transcript.len() as u64);
    for call in &site.transcript {
        encode_call(&mut out, call);
    }
    Some(out)
}

/// A cursor over a payload; every read is `None` at the first thing that is not
/// the grammar, and the whole table is then a miss.
struct Reader<'t> {
    text: &'t str,
    at: usize,
}

impl<'t> Reader<'t> {
    fn byte(&mut self) -> Option<u8> {
        let byte = *self.text.as_bytes().get(self.at)?;
        self.at += 1;
        Some(byte)
    }

    fn until(&mut self, terminator: u8) -> Option<&'t str> {
        let rest = self.text.get(self.at..)?;
        let end = rest.bytes().position(|byte| byte == terminator)?;
        self.at += end + 1;
        rest.get(..end)
    }

    fn number(&mut self) -> Option<u64> {
        self.until(b';')?.parse().ok()
    }

    fn count(&mut self) -> Option<usize> {
        let count = usize::try_from(self.number()?).ok()?;
        // A count cannot exceed the bytes left (every item takes at least one),
        // which bounds an allocation a damaged file could ask for.
        (count <= self.text.len().saturating_sub(self.at)).then_some(count)
    }

    fn string(&mut self) -> Option<String> {
        let length: usize = self.until(b':')?.parse().ok()?;
        let value = self.text.get(self.at..self.at.checked_add(length)?)?;
        self.at += length;
        Some(value.to_string())
    }

    fn strings(&mut self) -> Option<Vec<String>> {
        let count = self.count()?;
        (0..count).map(|_| self.string()).collect()
    }

    fn result<T>(&mut self, ok: impl FnOnce(&mut Self) -> Option<T>) -> Option<Result<T, String>> {
        match self.byte()? {
            b'o' => Some(Ok(ok(self)?)),
            b'x' => Some(Err(self.string()?)),
            _ => None,
        }
    }

    fn value(&mut self) -> Option<ConstValue> {
        Some(match self.byte()? {
            b'u' => ConstValue::Undefined,
            b'z' => ConstValue::Null,
            b't' => ConstValue::Bool(true),
            b'f' => ConstValue::Bool(false),
            b'n' => ConstValue::Number(f64::from_bits(
                u64::from_str_radix(self.until(b';')?, 16).ok()?,
            )),
            b'b' => ConstValue::BigInt(self.until(b';')?.parse().ok()?),
            b's' => ConstValue::Str(self.string()?),
            tag @ (b'a' | b'e') => {
                let count = self.count()?;
                let items = (0..count)
                    .map(|_| self.value())
                    .collect::<Option<Vec<_>>>()?;
                if tag == b'a' {
                    ConstValue::Array(items)
                } else {
                    ConstValue::Set(items)
                }
            }
            b'm' => {
                let count = self.count()?;
                let mut entries = Vec::with_capacity(count);
                for _ in 0..count {
                    entries.push((self.value()?, self.value()?));
                }
                ConstValue::Map(entries)
            }
            _ => return None,
        })
    }

    fn call(&mut self) -> Option<ChannelCall> {
        Some(match self.byte()? {
            b'R' => {
                let path = self.string()?;
                ChannelCall::Read(path, self.result(|reader| reader.string())?)
            }
            b'B' => {
                let path = self.string()?;
                let target = match self.byte()? {
                    b'+' => Some(self.string()?),
                    b'-' => None,
                    _ => return None,
                };
                ChannelCall::Bundle(path, target, self.result(|reader| reader.string())?)
            }
            b'D' => {
                let path = self.string()?;
                let recursive = match self.byte()? {
                    b't' => true,
                    b'f' => false,
                    _ => return None,
                };
                ChannelCall::ReadDir(path, recursive, self.result(|reader| reader.strings())?)
            }
            b'G' => {
                let path = self.string()?;
                ChannelCall::Digest(
                    path,
                    self.result(|reader| Some((reader.string()?, reader.number()?)))?,
                )
            }
            b'S' => ChannelCall::Stage(self.string()?, self.string()?, self.string()?),
            b'T' => {
                let kind = self.string()?;
                ChannelCall::Staged(kind, self.result(|reader| reader.strings())?)
            }
            _ => return None,
        })
    }
}

fn decode_site(payload: &str) -> Option<CachedSite> {
    let mut reader = Reader {
        text: payload,
        at: 0,
    };
    let value = reader.value()?;
    let asset_count = reader.count()?;
    let mut assets = Vec::with_capacity(asset_count);
    for _ in 0..asset_count {
        assets.push(EmittedAsset {
            kind: reader.string()?,
            key: reader.string()?,
            line: reader.string()?,
        });
    }
    let scheduled = reader.strings()?;
    let fuel_used = reader.number()?;
    let call_count = reader.count()?;
    let mut transcript = Vec::with_capacity(call_count);
    for _ in 0..call_count {
        transcript.push(reader.call()?);
    }
    // Exactly the payload: trailing bytes mean it was not what this build wrote.
    (reader.at == payload.len()).then_some(CachedSite {
        run: SiteRun {
            value,
            assets,
            scheduled,
            fuel_used,
        },
        transcript,
    })
}

#[cfg(test)]
mod disk_tests {
    use super::*;

    fn site(value: ConstValue, transcript: Vec<ChannelCall>) -> CachedSite {
        CachedSite {
            run: SiteRun {
                value,
                assets: vec![EmittedAsset {
                    kind: "css".into(),
                    key: "k\nwith newline".into(),
                    line: "a { b: c; }".into(),
                }],
                scheduled: vec!["done();".into(), String::new()],
                fuel_used: 123_456,
            },
            transcript,
        }
    }

    fn round_trip(original: &CachedSite) -> CachedSite {
        let payload = encode_site(original).expect("plain data encodes");
        decode_site(&payload).expect("what was encoded decodes")
    }

    #[test]
    fn every_plain_value_and_every_channel_call_survives_the_trip() {
        let value = ConstValue::Array(vec![
            ConstValue::Undefined,
            ConstValue::Null,
            ConstValue::Bool(true),
            ConstValue::Bool(false),
            ConstValue::Number(1.5),
            ConstValue::Number(-0.0),
            ConstValue::Number(f64::INFINITY),
            ConstValue::Number(f64::MIN_POSITIVE),
            ConstValue::BigInt(i128::MIN),
            ConstValue::BigInt(i128::MAX),
            ConstValue::Str("unicode: \u{1F600} \u{e9}; with ; and : and \n".into()),
            ConstValue::Str(String::new()),
            ConstValue::Set(vec![ConstValue::Number(1.0), ConstValue::Str("x".into())]),
            ConstValue::Map(vec![(
                ConstValue::Str("k".into()),
                ConstValue::Array(vec![ConstValue::Map(Vec::new())]),
            )]),
        ]);
        let transcript = vec![
            ChannelCall::Read("a.txt".into(), Ok("text; with : stuff\n".into())),
            ChannelCall::Read("gone".into(), Err("no such file".into())),
            ChannelCall::Bundle(
                "b.png".into(),
                Some("/b.png".into()),
                Ok("/b.hash.png".into()),
            ),
            ChannelCall::Bundle("c".into(), None, Err("nope".into())),
            ChannelCall::ReadDir("d".into(), true, Ok(vec!["x".into(), "y z".into()])),
            ChannelCall::ReadDir("e".into(), false, Ok(Vec::new())),
            ChannelCall::Digest("f".into(), Ok(("abc123".into(), 42))),
            ChannelCall::Digest("g".into(), Err("unreadable".into())),
            ChannelCall::Stage("css".into(), "tok".into(), ".tok { a: b }".into()),
            ChannelCall::Staged("css".into(), Ok(vec![".tok { a: b }".into()])),
            ChannelCall::Staged("js".into(), Err("none".into())),
        ];
        let original = site(value, transcript);
        let back = round_trip(&original);
        // `-0.0 == 0.0` and `NaN != NaN` under `PartialEq`: the bits are the claim.
        assert_eq!(
            format!("{:?}", back.run.value),
            format!("{:?}", original.run.value)
        );
        assert_eq!(back.run.assets, original.run.assets);
        assert_eq!(back.run.scheduled, original.run.scheduled);
        assert_eq!(back.run.fuel_used, original.run.fuel_used);
        assert_eq!(back.transcript, original.transcript);
        let nan = site(
            ConstValue::Number(f64::from_bits(0x7ff8_0000_0000_1234)),
            Vec::new(),
        );
        match round_trip(&nan).run.value {
            ConstValue::Number(number) => assert_eq!(number.to_bits(), 0x7ff8_0000_0000_1234),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_closure_snapshot_names_an_entity_and_stays_in_memory() {
        let callable = ConstValue::Callable {
            body: crate::id::Id(7),
            captures: Vec::new(),
        };
        assert!(encode_site(&site(callable.clone(), Vec::new())).is_none());
        assert!(
            encode_site(&site(ConstValue::Array(vec![callable]), Vec::new())).is_none(),
            "nested too"
        );
    }

    #[test]
    fn anything_but_what_was_written_is_refused_rather_than_misread() {
        let payload = encode_site(&site(ConstValue::Str("hello".into()), Vec::new())).unwrap();
        for cut in 0..payload.len() {
            // A truncation can only land mid-grammar or on a char boundary the
            // writer never produced; none of them may decode to a DIFFERENT site.
            if let Some(prefix) = payload.get(..cut) {
                assert!(decode_site(prefix).is_none(), "{cut}: {prefix:?}");
            }
        }
        assert!(
            decode_site(&format!("{payload}x")).is_none(),
            "trailing bytes"
        );
        assert!(decode_site("a99999999999;").is_none(), "an absurd count");
        assert!(decode_site("s5:ab").is_none(), "a string past the end");
    }
}
