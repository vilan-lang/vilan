//! `vilan build --explain` (backlog G11) — the build's own account of what it
//! wrote, and of what would make it write again.
//!
//! The complaint the verb answers: the const channel's contributions are
//! scattered across files *by construction* — that scatter IS import-driven
//! composition — so "where did this `dist/` file come from?" is a grep today,
//! and for an accumulated kind a grep over every `emit` site. The compiler held
//! the answer the whole time and never said it: every channel call knows its
//! `const` site, the flush knows which kind each line landed in, the copy knows
//! which file it carried, and the hook runner knows what ran and what was
//! `Fresh`.
//!
//! **No new machinery and no second source of truth.** This module records
//! nothing the build did not already do; it is a log the build's own writers
//! append to as they write, plus [`vilan_core::const_eval::ConstFact`], the one
//! record extension the feature needed (a `const` site's location beside each
//! channel fact — the operational halves are deduplicated and sorted precisely
//! so a build's bytes never depend on call order, which is the property that
//! erases who contributed). Every line below is read back out of that log. If
//! the build stops writing something, the report stops naming it, because the
//! report is written by the same call that writes the file.
//!
//! **The shape is a contract.** Line-oriented, one `output` / `input` block per
//! file, every detail line a fixed key — the refusal doctrine's cousin: a build
//! that refuses by name should explain by name, and in a shape `grep` and a diff
//! can both read. Paths print exactly as the build's own `Compiled` / `Emitted`
//! / `Bundled` lines print them, so a reader can match the two by eye.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether `--explain` was asked for. Checked before every record, so a build
/// without the flag pays one relaxed load per written file and nothing else.
static ASKED: AtomicBool = AtomicBool::new(false);

/// This build's log. Process-global for the reason the watcher's recorded-input
/// set is: the writers that append sit several opaque frames below the command,
/// and threading a sink through `write_assets` / `write_bundled` /
/// `write_chunks` would put a parameter for a report on functions whose job is
/// to write files.
///
/// The lock RECOVERS from poisoning, this tree's one posture (backlog E97): a
/// report is not worth turning a panicking build into a second panic.
static LOG: Mutex<Log> = Mutex::new(Log::new());

#[derive(Default)]
struct Log {
    outputs: Vec<Output>,
    facts: Vec<LoggedFact>,
    hooks: Vec<Hook>,
}

impl Log {
    const fn new() -> Log {
        Log {
            outputs: Vec::new(),
            facts: Vec::new(),
            hooks: Vec::new(),
        }
    }
}

/// One file this build wrote, and what it is.
struct Output {
    path: PathBuf,
    /// The leg whose build owns it — the stem `dist/<leg>.<ext>` is named
    /// after. Empty for a file no leg owns (a hook's declared output).
    leg: String,
    role: Role,
}

/// What a written file IS. The names are the ones the build already uses for
/// them: [`crate::LegNamespace::claims`] answers with these very phrases when
/// it refuses to let a bundled resource take one of the names, so the report
/// and the refusal describe a `dist/` in one vocabulary.
enum Role {
    /// `<leg>.<kind>` — the flush of an accumulated emit kind.
    Emitted { kind: String },
    /// A file `asset::bundle` / `asset::bundle_as` carried into the output
    /// directory, under the name it took there.
    Bundled { source: PathBuf, name: String },
    /// `<leg>.<ext>` — the leg's JavaScript.
    Bundle,
    /// `<leg>.chunks.json`.
    Manifest,
    /// `<leg>.<arm>.js` — one route chunk of a `split = true` browser leg.
    Chunk { arm: String },
    /// A `[[build.hook]]`'s declared `outputs` entry.
    HookOutput { hook: String },
}

impl Role {
    /// The `role` line's text.
    fn name(&self) -> String {
        match self {
            Role::Emitted { kind } => format!("emitted kind `{kind}`"),
            Role::Bundled { .. } => "bundled copy".to_string(),
            Role::Bundle => "compiled bundle".to_string(),
            Role::Manifest => "build manifest".to_string(),
            Role::Chunk { .. } => "route chunk".to_string(),
            Role::HookOutput { .. } => "hook output".to_string(),
        }
    }

    /// A sort key that keeps two roles on one path in a fixed order. Paths are
    /// the primary key; this only has to be total and stable.
    fn order(&self) -> u8 {
        match self {
            Role::Bundle => 0,
            Role::Emitted { .. } => 1,
            Role::Manifest => 2,
            Role::Chunk { .. } => 3,
            Role::Bundled { .. } => 4,
            Role::HookOutput { .. } => 5,
        }
    }
}

/// One const-channel fact, resolved: the site as `file:line`, and what it did.
/// The resolution happens in `compile_to_js`, which is the only place holding
/// both the program and the source text a line number is counted in.
pub struct Fact {
    /// `src/client.vl:16` — the `const` expression, located exactly as a
    /// const-eval diagnostic would locate it.
    pub site: String,
    pub what: FactKind,
}

/// A [`Fact`] with the leg that compiled it. The leg is what keeps a site in
/// `src/client.vl` from being reported as a contributor to `dist/server.css`:
/// a fact carries a source location and no leg, and one `dist/` holds several
/// legs' flushes of the same kind. It is attached on the way in — by the
/// caller that has just compiled that leg — rather than carried through
/// `compile_to_js`, which is handed an entry and does not know the output's
/// name.
struct LoggedFact {
    leg: String,
    site: String,
    what: FactKind,
}

/// What a [`Fact`] records — [`vilan_core::const_eval::ConstFactKind`] with its
/// site resolved and the pieces the report does not print dropped.
pub enum FactKind {
    Emitted {
        kind: String,
    },
    Bundled {
        name: String,
        /// `asset::bundle` or `asset::bundle_as` — the spelling the program
        /// wrote. It is the difference between "this file is here because its
        /// path put it here" and "this url was chosen at the call", which is
        /// the first thing a reader wants when a copy is somewhere surprising.
        function: String,
    },
    Read {
        path: PathBuf,
        function: String,
    },
}

/// One `[[build.hook]]` and this build's verdict for it.
struct Hook {
    name: String,
    /// `ran` or `Fresh` — the second is the word the build already prints when
    /// it skips one, so the report and the terminal agree.
    verdict: &'static str,
    inputs: Vec<PathBuf>,
    outputs: Vec<PathBuf>,
}

/// Turns the verb on. Called once, from the flag.
pub fn ask() {
    ASKED.store(true, Ordering::Relaxed);
}

pub fn asked() -> bool {
    ASKED.load(Ordering::Relaxed)
}

fn log() -> std::sync::MutexGuard<'static, Log> {
    LOG.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Starts a build's record over. Every round of `--watch` explains itself, so
/// a round must not inherit the previous one's outputs.
pub fn begin() {
    if !asked() {
        return;
    }
    *log() = Log::new();
}

/// Records one written file. Every caller is a place that just wrote it — the
/// report cannot name a file the build did not produce, because the same call
/// produces it.
fn record(path: PathBuf, leg: &str, role: Role) {
    if !asked() {
        return;
    }
    log().outputs.push(Output {
        path,
        leg: leg.to_string(),
        role,
    });
}

pub fn emitted(path: PathBuf, leg: &str, kind: &str) {
    record(
        path,
        leg,
        Role::Emitted {
            kind: kind.to_string(),
        },
    );
}

pub fn bundled(path: PathBuf, leg: &str, source: PathBuf, name: &str) {
    record(
        path,
        leg,
        Role::Bundled {
            source,
            name: name.to_string(),
        },
    );
}

pub fn bundle(path: PathBuf, leg: &str) {
    record(path, leg, Role::Bundle);
}

pub fn manifest(path: PathBuf, leg: &str) {
    record(path, leg, Role::Manifest);
}

pub fn chunk(path: PathBuf, leg: &str, arm: &str) {
    record(
        path,
        leg,
        Role::Chunk {
            arm: arm.to_string(),
        },
    );
}

/// Records a `[[build.hook]]` and what this build did about it. `outputs` are
/// resolved paths, so they can be matched against the files everything else
/// records.
pub fn hook(name: &str, verdict: &'static str, inputs: Vec<PathBuf>, outputs: Vec<PathBuf>) {
    if !asked() {
        return;
    }
    for output in &outputs {
        log().outputs.push(Output {
            path: output.clone(),
            leg: String::new(),
            role: Role::HookOutput {
                hook: name.to_string(),
            },
        });
    }
    log().hooks.push(Hook {
        name: name.to_string(),
        verdict,
        inputs,
        outputs,
    });
}

/// Records one leg's const-channel provenance — see [`LoggedFact`].
pub fn leg_facts(leg: &str, facts: Vec<Fact>) {
    if !asked() {
        return;
    }
    let mut log = log();
    for fact in facts {
        log.facts.push(LoggedFact {
            leg: leg.to_string(),
            site: fact.site,
            what: fact.what,
        });
    }
}

/// Prints the report, if one was asked for. Called from the build's success
/// paths only: a build that failed has not written the `dist/` it would be
/// explaining, and its diagnostics are the account it owes.
pub fn print() {
    if !asked() {
        return;
    }
    let log = log();
    print!("{}", render(&log));
}

/// The report as text. Separated from the printing so it is one pure function
/// over the log — the thing a test can reason about.
fn render(log: &Log) -> String {
    let mut out = String::new();
    let mut outputs: Vec<&Output> = log.outputs.iter().collect();
    outputs.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.role.order().cmp(&right.role.order()))
    });
    outputs
        .dedup_by(|left, right| left.path == right.path && left.role.order() == right.role.order());
    for output in &outputs {
        out.push_str(&format!("output  {}\n", output.path.display()));
        line(&mut out, "role", &output.role.name());
        match &output.role {
            Role::Emitted { kind } => {
                for site in sites_emitting(log, &output.leg, kind) {
                    line(&mut out, "emitted", &site);
                }
            }
            Role::Bundled { source, name } => {
                line(&mut out, "source", &source.display().to_string());
                for site in sites_bundling(log, &output.leg, name) {
                    line(&mut out, "named", &site);
                }
            }
            Role::Bundle | Role::Manifest => line(&mut out, "leg", &output.leg),
            Role::Chunk { arm } => {
                line(&mut out, "leg", &output.leg);
                line(&mut out, "arm", arm);
            }
            Role::HookOutput { hook } => {
                let verdict = log
                    .hooks
                    .iter()
                    .find(|declared| &declared.name == hook)
                    .map_or("ran", |declared| declared.verdict);
                line(&mut out, "hook", &format!("{hook} ({verdict})"));
            }
        }
        out.push('\n');
    }
    for (path, detail) in inputs(log) {
        out.push_str(&format!("input   {}\n", path.display()));
        for read in &detail.read {
            line(&mut out, "read", read);
        }
        for declared in &detail.declared {
            line(&mut out, "declared", declared);
        }
        for invalidated in &detail.invalidates {
            line(&mut out, "invalidates", &invalidated.display().to_string());
        }
        out.push('\n');
    }
    out
}

/// One detail line: two spaces, the key padded to a fixed column, the value.
/// Fixed because the report is meant to be read by a person scanning a column
/// and by a `grep` matching a prefix, and both want the key to start and stop
/// in the same place on every line.
fn line(out: &mut String, key: &str, value: &str) {
    out.push_str(&format!("  {key:<11}  {value}\n"));
}

/// Every `const` site that contributed to `leg`'s flush of `kind`, sorted and
/// deduplicated — a site that emitted four hundred rules into one sheet
/// contributed to it once.
fn sites_emitting(log: &Log, leg: &str, kind: &str) -> Vec<String> {
    let mut sites: Vec<String> = log
        .facts
        .iter()
        .filter(|fact| fact.leg == leg)
        .filter_map(|fact| match &fact.what {
            FactKind::Emitted { kind: emitted } if emitted == kind => Some(fact.site.clone()),
            _ => None,
        })
        .collect();
    sites.sort();
    sites.dedup();
    sites
}

/// Every `const` site that named the copy `leg` wrote under `name`, with the
/// spelling it used. Two sites naming one copy are two lines: the registry
/// deduplicates a name and the provenance does not.
fn sites_bundling(log: &Log, leg: &str, name: &str) -> Vec<String> {
    let mut sites: Vec<String> = log
        .facts
        .iter()
        .filter(|fact| fact.leg == leg)
        .filter_map(|fact| match &fact.what {
            FactKind::Bundled {
                name: bundled,
                function,
            } if bundled == name => Some(format!("{} ({function})", fact.site)),
            _ => None,
        })
        .collect();
    sites.sort();
    sites.dedup();
    sites
}

/// What one tracked input's block says.
#[derive(Default)]
struct InputDetail {
    /// `src/client.vl:16 (asset::read)` — a const site that read it.
    read: Vec<String>,
    /// `[[build.hook]] icons` — a hook that declared it.
    declared: Vec<String>,
    /// The outputs a change to it would move.
    invalidates: Vec<PathBuf>,
}

/// Every tracked input, with what reads it and what it invalidates.
///
/// **What "invalidates" means here, exactly.** Three answers, and each one is
/// read off a record rather than reasoned about:
///
/// * A const input's leg's **compiled bundle**, always. The channel's inputs
///   are sources to the compile — `Compiled::sources` chains them in, which is
///   what makes the watch loop recompile a leg whose asset moved — so a change
///   to one moves the bundle whether or not its bytes reach any `dist/` file
///   by another road. This is the line a `digest` input would otherwise have
///   none of: its value is serialized into the bundle and nowhere else.
/// * The **flushes and copies of the sites that read it**, which is the
///   precise half: a `bundle` source moves its copy, and a `read` feeding a
///   site that emits moves that site's kind.
/// * For a hook input, that **hook's declared outputs** — the hook's own
///   statement of what it writes, which is the same declaration its freshness
///   stamp and the watcher read.
///
/// Deliberately not the build manifest, which every build of a browser leg
/// rewrites whatever moved: naming it under every input would be true and
/// would say nothing.
fn inputs(log: &Log) -> Vec<(PathBuf, InputDetail)> {
    let mut by_path: BTreeMap<PathBuf, InputDetail> = BTreeMap::new();
    for fact in &log.facts {
        let FactKind::Read { path, function } = &fact.what else {
            continue;
        };
        let detail = by_path.entry(path.clone()).or_default();
        detail.read.push(format!("{} ({function})", fact.site));
        for output in &log.outputs {
            let compiled_bundle = output.leg == fact.leg && matches!(output.role, Role::Bundle);
            if compiled_bundle || produced_at(log, &fact.leg, &fact.site, output) {
                detail.invalidates.push(output.path.clone());
            }
        }
    }
    for hook in &log.hooks {
        for input in &hook.inputs {
            let detail = by_path.entry(input.clone()).or_default();
            detail
                .declared
                .push(format!("`[[build.hook]]` {}", hook.name));
            detail.invalidates.extend(hook.outputs.iter().cloned());
        }
    }
    by_path
        .into_iter()
        .map(|(path, mut detail)| {
            detail.read.sort();
            detail.read.dedup();
            detail.declared.sort();
            detail.declared.dedup();
            detail.invalidates.sort();
            detail.invalidates.dedup();
            (path, detail)
        })
        .collect()
}

/// Whether `output` is something the `const` site at `site` (on `leg`)
/// produced — the join that turns "this input was read here" into "so this file
/// would move".
fn produced_at(log: &Log, leg: &str, site: &str, output: &Output) -> bool {
    if output.leg != leg {
        return false;
    }
    log.facts.iter().any(|fact| {
        fact.leg == leg
            && fact.site == site
            && match (&fact.what, &output.role) {
                (FactKind::Emitted { kind }, Role::Emitted { kind: written }) => kind == written,
                (FactKind::Bundled { name, .. }, Role::Bundled { name: written, .. }) => {
                    name == written
                }
                _ => false,
            }
    })
}
