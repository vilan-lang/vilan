use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Mutex,
    time::{Duration, Instant, SystemTime},
};

use ariadne::{Color, Label, Report, ReportKind, sources};
use clap::{Parser as _, Subcommand};
mod bindgen;
mod explain;
mod hmr;
mod init;
mod job;
mod paint;
mod upgrade;
mod watch_log;

use job::ManagedChild;
use vilan_core::analyzer::{Program, SourceId, analyze, check_library_contract};
use vilan_core::manifest::Package;
use vilan_core::transformer::{EmittedChunk, transform};
use vilan_core::{Backend, BuildOptions, Manifest, Platform, Workspace};

/// The vilan language toolchain.
#[derive(clap::Parser)]
#[command(
    name = "vilan",
    version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("VILAN_BUILD_SHA"), ")"),
    about
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a ready-to-run project: a manifest, sources that compile, and a
    /// `.gitignore`. Creates no repository — `git init` stays yours.
    Init {
        /// The directory to create (it must not exist, or must be empty).
        /// Omitted: scaffold into the current directory, which must not already
        /// hold a `vilan.toml`.
        name: Option<String>,
        /// Which scaffold to write: `node` (a package that runs on node),
        /// `browser` (a reactive browser app), or `fullstack` (one package,
        /// two entries — a browser client and a node server). Omitted: choose
        /// at a prompt; without a terminal that is an error, never a hang.
        #[arg(long)]
        template: Option<String>,
    },
    /// Compile to JavaScript, writing `<file>.js`. With no path, compiles the
    /// project entry from the nearest `vilan.toml`.
    Build {
        /// A `.vl` file, a project directory, or omitted to use `vilan.toml`.
        file: Option<PathBuf>,
        /// Print the JavaScript to stdout instead of writing `<file>.js`.
        #[arg(long)]
        stdout: bool,
        /// Also report the route-chunk plan — what a split build would load
        /// lazily per route arm (proposal/bundle-splitting.md). Analysis only;
        /// the emitted JavaScript is unchanged.
        #[arg(long)]
        print_chunks: bool,
        /// The platform to build for: `node` (`node:24`), `deno` (`deno:2`), `bun`
        /// (`bun:1`), `browser`, or `none`. Overrides the package's `target`; defaults
        /// to it, else `node`. `--target` is an accepted alias.
        #[arg(long, alias = "target")]
        platform: Option<String>,
        /// The emitter backend: `js` (the only backend today).
        #[arg(long)]
        backend: Option<String>,
        /// Also emit debug dumps beside the source, one per pipeline stage:
        /// `.parse-raw.out` (the tree the parser produced), `.parse.out` (the
        /// same tree after the `css` / element / lift desugars — what analysis
        /// receives), `.analyze.out` (the analyzed program) and
        /// `.callgraph.out` (the call graph the post-analysis passes shared).
        #[arg(short, long)]
        debug: bool,
        /// Rebuild whenever a watched `.vl` source file changes (Ctrl-C to stop).
        #[arg(long)]
        watch: bool,
        /// Run every `[[build.hook]]` even if it is fresh. Freshness compares
        /// the hook's declared `inputs` and `outputs`, so this is the escape
        /// for a hook that reads something it did not declare.
        #[arg(long)]
        rerun_hooks: bool,
        /// After the build, print what wrote every file in the output
        /// directory — the emitting `const` sites of each accumulated kind,
        /// the source and the naming site of each bundled copy, the hook
        /// behind each declared output — and, per tracked input, what a
        /// change to it would move. Builds first: explaining a stale tree
        /// would lie.
        #[arg(long)]
        explain: bool,
    },
    /// Type-check, reporting diagnostics without writing output. With no path,
    /// checks the project entry from the nearest `vilan.toml`.
    Check {
        /// A `.vl` file, a project directory, or omitted to use `vilan.toml`.
        file: Option<PathBuf>,
        /// The platform to check for: `node` (`node:24`), `deno` (`deno:2`), `bun`
        /// (`bun:1`), `browser`, or `none`. Overrides the package's `target`; defaults
        /// to it, else `node`. `--target` is an accepted alias.
        #[arg(long, alias = "target")]
        platform: Option<String>,
        /// The emitter backend: `js` (the only backend today).
        #[arg(long)]
        backend: Option<String>,
        /// Also emit debug dumps beside the source, one per pipeline stage:
        /// `.parse-raw.out` (the tree the parser produced), `.parse.out` (the
        /// same tree after the `css` / element / lift desugars — what analysis
        /// receives), `.analyze.out` (the analyzed program) and
        /// `.callgraph.out` (the call graph the post-analysis passes shared).
        #[arg(short, long)]
        debug: bool,
        /// Re-check whenever a watched `.vl` source file changes (Ctrl-C to stop).
        #[arg(long)]
        watch: bool,
    },
    /// Build and run a source file, forwarding any trailing arguments to the
    /// program (reach them with `import std::process;` and `process::args()`).
    Run {
        /// A `.vl` file, a project directory, or omitted to use `vilan.toml`.
        file: Option<PathBuf>,
        /// Rebuild and restart whenever a watched `.vl` source file changes. Place it
        /// before the file (`vilan run --watch app.vl`), ahead of any program args.
        #[arg(long)]
        watch: bool,
        /// Turn off hot module replacement under `--watch` (plain restart-the-server
        /// behavior). HMR is otherwise on for a workspace with a browser leg.
        #[arg(long)]
        no_hmr: bool,
        /// The `127.0.0.1` port for the HMR dev channel (`0` ⇒ an OS-assigned
        /// ephemeral port). Only meaningful with `--watch` on an HMR-eligible project.
        #[arg(long, default_value_t = hmr::DEFAULT_HMR_PORT)]
        hmr_port: u16,
        /// In a workspace with more than one `node` package, which one to run (by
        /// package name). The others still compile as part of the workspace but
        /// are not launched. Unnecessary for a single-node workspace.
        #[arg(long)]
        entry: Option<String>,
        /// Arguments passed through to the running program (after the file).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Format vilan source files in place. Already-formatted (and any not-yet
    /// formattable) files are left untouched.
    Fmt {
        /// Files or directories to format. Defaults to the current directory.
        paths: Vec<PathBuf>,
        /// Report files that would change without rewriting them (exit 1 if any).
        #[arg(long)]
        check: bool,
    },
    /// Run `*_test.vl` tests (each passes by exiting 0; a failed `assert` panics).
    Test {
        /// A test file, a directory of tests, or omitted to use the project root.
        path: Option<PathBuf>,
        /// Re-run the tests whenever a watched `.vl` source file changes (Ctrl-C to stop).
        #[arg(long)]
        watch: bool,
    },
    /// Generate `external` bindings from a TypeScript declaration file. The
    /// emitted `.vl` is ordinary source to review and commit — never a build
    /// step, and nothing regenerates it behind your back.
    Bindgen {
        /// The `.d.ts` file to read.
        file: PathBuf,
        /// Where to write the bindings. Omitted: `<file-stem>.vl` beside the
        /// input (`leaflet.d.ts` → `leaflet.vl`).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// The `[platform("…")]` fence stamped on every generated binding:
        /// `node`, `deno`, `bun`, `browser`, or `@process`. Required — a wrong
        /// guess baked into checked-in source is worse than choosing once.
        #[arg(long)]
        platform: String,
        /// Emit only this declaration and everything its signatures reach —
        /// base types, member types, parameter and return types. Repeatable.
        /// Omitted: the whole file.
        #[arg(long)]
        only: Vec<String>,
        /// Print the bindings instead of writing a file.
        #[arg(long)]
        stdout: bool,
        /// Also report coverage: how many declarations and members bound, and
        /// which TypeScript constructs did not.
        #[arg(long)]
        stats: bool,
    },
    /// Update this binary (and `vilan-lsp` beside it) to the newest release.
    /// This is the only command that touches the network.
    Upgrade {
        /// Report whether a newer release exists without changing anything.
        #[arg(long)]
        check: bool,
    },
}

fn main() -> ExitCode {
    // Compilation recurses over deeply-nested ASTs and type graphs, which can
    // run past the default main-thread stack on otherwise-valid programs. Do the
    // work on a worker with a generous stack, as rustc and other compilers do;
    // the reservation is virtual address space, so it costs nothing unless used.
    //
    // The margin is measured, not folklore, and every recursive family behind
    // it is BOUNDED now (B138/B139/B142, `VILAN_DEPTH_STATS`) — which is what
    // brought this number down from 256 MiB:
    //
    //   * the PARSER, at 500 levels of nesting. The deepest consumer in the
    //     pipeline and the one that runs first, so before B142 it reached the
    //     cliff before either analyzer bound could refuse. Measured through
    //     this binary on the worst plant (5000 nested parentheses): peak depth
    //     501, 35.2 MiB unoptimized, ~10 MiB optimized.
    //   * the phase-1 expression walk, ~36 KiB per level (500 levels, ~18 MiB).
    //   * the return-inference chain, ~12.8 KiB per call link (500, ~6.4 MiB).
    //
    // Each refuses with a diagnostic rather than overflowing, and the phases
    // run in SEQUENCE — the parse has unwound before analysis starts — so the
    // worst case is the largest of them, not their sum: ~35 MiB unoptimized.
    // Real code is nowhere near it: all 211 corpus entries peak at 23 parser
    // levels against a bound of 500, and a realistic analysis peaks under 1 MiB.
    //
    // 128 MiB is ~3.6x that measured worst case, and the headroom is not idle.
    // A macro-world compile NESTS a full pipeline inside the running analysis
    // (see `Document::analyze` in vilan-lsp), so a deep walk carrying a deep
    // nested parse inside it composes to roughly 53 MiB; this covers that with
    // room over. Bounding the parser is what made the number finite at all —
    // before B142 there was no worst case to size anything against, and the
    // margin was standing in for a bound that did not exist.
    const COMPILER_STACK_SIZE: usize = 128 * 1024 * 1024;
    std::thread::Builder::new()
        .stack_size(COMPILER_STACK_SIZE)
        .spawn(run_cli)
        .expect("spawn compiler thread")
        .join()
        .expect("compiler thread panicked")
}

fn run_cli() -> ExitCode {
    match Cli::parse().command {
        Command::Build {
            file,
            stdout,
            print_chunks,
            platform,
            backend,
            debug,
            watch,
            rerun_hooks,
            explain,
        } => match effective_backend(backend.as_deref()) {
            Err(message) => report_error(&message),
            // `--stdout` prints a bundle, not a build: it writes no output
            // directory at all, and its one stream is the JavaScript. There
            // would be nothing to explain, and the report would corrupt what
            // the stream is for. Refused rather than quietly dropped.
            Ok(_backend) if explain && stdout => report_error(
                "`--explain` reports what a build wrote, and `--stdout` writes nothing — \
                 it prints a bundle, not a build. Drop one of the two.",
            ),
            Ok(_backend) => {
                PRINT_CHUNKS.store(print_chunks, std::sync::atomic::Ordering::Relaxed);
                if explain {
                    explain::ask();
                }
                let roots = watch.then(|| watch_roots(&file));
                // M22: the per-leg reuse record lives for the life of the
                // watcher and is created only for one. A one-shot build passes
                // `None` and is the build it always was.
                let mut watch_state = watch.then(BuildWatchState::default);
                run_or_watch(roots, move || {
                    build_once(
                        file.clone(),
                        stdout,
                        platform.clone(),
                        debug,
                        rerun_hooks,
                        watch_state.as_mut(),
                    )
                })
            }
        },
        Command::Check {
            file,
            platform,
            backend,
            debug,
            watch,
        } => match effective_backend(backend.as_deref()) {
            Err(message) => report_error(&message),
            Ok(_backend) => {
                let roots = watch.then(|| watch_roots(&file));
                run_or_watch(roots, move || {
                    check_once(file.clone(), platform.clone(), debug)
                })
            }
        },
        // `run`/`test` execute with `node`. `run --watch` restarts the process on a
        // change (see `run_watch`); the others just re-run the command.
        Command::Run {
            file,
            args,
            watch,
            no_hmr,
            hmr_port,
            entry,
        } => {
            if watch {
                run_watch(file, args, no_hmr, hmr_port, entry)
            } else {
                run_once(file, &args, entry.as_deref())
            }
        }
        Command::Test { path, watch } => {
            let roots = watch.then(|| watch_roots(&path));
            run_or_watch(roots, move || test(path.clone()))
        }
        Command::Fmt { paths, check } => fmt(&paths, check),
        Command::Init { name, template } => init::init(name, template),
        Command::Bindgen {
            file,
            output,
            platform,
            only,
            stdout,
            stats,
        } => bindgen::bindgen(file, output, platform, only, stdout, stats),
        Command::Upgrade { check } => upgrade::upgrade(check),
    }
}

/// Builds the project once (a lone package / bare file for its `--platform`, a
/// workspace for each member's platform; a `[library]` isn't buildable).
fn build_once(
    file: Option<PathBuf>,
    stdout: bool,
    platform: Option<String>,
    debug: bool,
    rerun_hooks: bool,
    // `Some` only under `--watch` (backlog M22): what the previous round
    // compiled each leg from, so a leg the edit did not reach is reused.
    watch_state: Option<&mut BuildWatchState>,
) -> RoundOutcome {
    // Before the hooks, which are the first thing that records: every round of
    // `--watch` explains itself, and a round must not inherit the previous
    // one's outputs.
    explain::begin();
    with_project(file, |project| {
        if let Err(outcome) = run_build_hooks(&project, rerun_hooks) {
            return outcome;
        }
        match project {
            Project::Single {
                unit,
                platform: package_platform,
                ..
            } => match effective_platform(platform.as_deref(), package_platform) {
                Ok(Platform::None) => no_host_platform(),
                Ok(platform) => build_single(&unit, stdout, platform, debug),
                Err(message) => report_error(&message),
            },
            // A workspace builds each member for its own declared platform, so the
            // `--platform` flag doesn't apply.
            Project::Workspace { root, members, .. } => {
                build_workspace(&root, &members, debug, watch_state)
            }
            Project::Library { name, .. } => not_buildable_library(&name),
        }
    })
}

/// Runs the project's build hooks before it is built (A9), reporting and
/// failing on the first that fails. `vilan check` deliberately doesn't call
/// this: it produces no artifacts, so there is nothing for a hook to feed.
///
/// `rerun` is `vilan build --rerun-hooks`: run every declared hook whether or
/// not it is fresh. It is the escape hatch `build-hooks.md` §3.2 names for the
/// staleness predicate's one accepted unsoundness — a hook that reads a file it
/// did not declare.
fn run_build_hooks(project: &Project, rerun: bool) -> Result<(), RoundOutcome> {
    let Some(hooks) = project.hooks() else {
        return Ok(());
    };
    // Before the hooks that DO run, because it is a statement about the ones
    // that do not — and because a first-party hook failing must not swallow it.
    note_refused_dependency_hooks(project);
    hooks.run(rerun).map_err(|message| {
        eprintln!("{} {message}", paint::error_prefix());
        RoundOutcome::Failed
    })
}

/// Says, once per build, that a dependency asked for build-time execution and
/// did not get it — tier 2's boundary made audible (`build-trust.md` §3,
/// `build-hooks.md` §4.3, ruled as Q4 on 2026-08-28).
///
/// Until now this was **silent**: `Project::hooks` reads the addressed
/// manifest and no other, so a dependency's `[build] run` produced no output,
/// no warning and no note (the paper's probe P5). "Absent means no" and "the
/// toolchain never looked" are indistinguishable from the terminal, and the
/// two readers that silence fails are the two who most need the line — one
/// debugs the dependency, the shell and their PATH before suspecting a policy
/// nobody told them about; the other never learns that a package in their
/// graph is asking to run code on their machine.
///
/// A **note, never a warning**, in either case. §3's own words forbid making
/// the refusal an error to be dismissed, and the opted-in case is not a
/// mistake either — it is a correct, forward-looking declaration the toolchain
/// simply does not honor yet. Neither line changes the exit code.
fn note_refused_dependency_hooks(project: &Project) {
    let mut reported: BTreeMap<PathBuf, vilan_core::manifest::DependencyHooks> = BTreeMap::new();
    for unit in project.units() {
        let Some(package_dir) = &unit.package_dir else {
            continue;
        };
        // A resolution failure is the COMPILE's to report, with its own
        // message and its own exit code. Reading it here is only how the
        // question gets asked, so a failure means the question goes
        // unanswered — never that the build says something twice.
        //
        // **Cache-only, deliberately**, where the compile that follows fetches:
        // asking a question must not reorder the build. Under the fetching
        // policy this pass would pull a git dependency over the network
        // *before* the first-party hooks run, and a hook that prepares the
        // environment a fetch needs would suddenly run too late. The cost is
        // that a not-yet-fetched dependency goes unmentioned on the build that
        // first fetches it, and is named on the next one — a note arriving one
        // build later, against a hook that never ran either way.
        let Ok((_, declarations)) = vilan_core::manifest::resolve_workspace_with_hook_report(
            package_dir,
            &git_deps_cached(),
        ) else {
            continue;
        };
        for declaration in declarations {
            // Once per build, not once per member: two legs depending on the
            // same package name it once, and a grant written by either one
            // counts.
            reported
                .entry(vilan_core::util::canonical_path(&declaration.directory))
                .and_modify(|recorded| recorded.opted_in |= declaration.opted_in)
                .or_insert(declaration);
        }
    }
    for declaration in reported.values() {
        let line = if declaration.opted_in {
            format!(
                "note: `{}` declares build hooks and is opted in (`build-hooks = true`), \
                 but no dependency's hooks run yet — this one did not.",
                declaration.name
            )
        } else {
            format!(
                "note: `{}` declares build hooks; they did not run. A dependency's build \
                 code needs `build-hooks = true` on its declaration — and even then, \
                 nothing runs it yet.",
                declaration.name
            )
        };
        eprintln!("{}", paint::err(paint::Style::DIM, &line));
    }
}

/// Type-checks the project once. A standalone `[library]` has no fixed platform, so
/// it verifies the platform contract (§4.2) instead of a single-platform build.
///
/// A file shared between the legs of a multi-entry package is checked under
/// EVERY color the build compiles it under, and every leg's diagnostics are
/// reported (E113) — the build compiles it once per leg and it must type-check
/// under each, so answering from one color would pass a file `vilan check .`
/// refuses. An explicit `--platform` overrides the lot: naming a platform is
/// asking about that platform.
fn check_once(file: Option<PathBuf>, platform: Option<String>, debug: bool) -> RoundOutcome {
    with_project(file, |project| match project {
        Project::Single {
            mut unit,
            platform: package_platform,
            shared_platforms,
            ..
        } => match effective_platform(platform.as_deref(), package_platform) {
            // A `none` package is a pure library — not buildable, but type-checkable
            // (against the base layer only).
            Ok(first) => {
                let goal = match unit.entry_mode {
                    vilan_core::EntryMode::Declared => CompileGoal::Check,
                    vilan_core::EntryMode::OpenFile { .. } => CompileGoal::CheckModule,
                };
                let mut platforms = vec![first];
                if platform.is_none() {
                    platforms.extend(shared_platforms);
                } else {
                    // The flag overrode the coloring, so it is also the whole
                    // answer to "why this platform" (E119). Nothing the file's
                    // own situation says still applies.
                    unit.platform_reasons = vec![(
                        first,
                        vilan_core::platform_color::PlatformReason::Flag.clause(),
                    )];
                }
                check_single(&unit, &platforms, debug, goal)
            }
            Err(message) => report_error(&message),
        },
        Project::Workspace { members, .. } => check_workspace(&members, debug),
        Project::Library { dir, name } => check_library(&dir, &name),
    })
}

/// Builds and runs the project once with Node, waiting for it to exit and
/// propagating its code (the blocking, non-`--watch` path). `entry` picks the
/// Node leg to run in a multi-node workspace (A15).
fn run_once(file: Option<PathBuf>, args: &[String], entry: Option<&str>) -> ExitCode {
    with_project(file, |project| {
        // `--rerun-hooks` is a `vilan build` flag: `run` is the dev loop, where
        // the whole point of the staleness gate is that an expensive hook stops
        // costing anything per round.
        if let Err(outcome) = run_build_hooks(&project, false) {
            return outcome.into();
        }
        match project {
            Project::Single { unit, platform, .. } => {
                let platform = platform.unwrap_or_default();
                if matches!(platform, Platform::Node { .. }) {
                    run_single(&unit, args)
                } else {
                    eprintln!(
                        "{} `vilan run` executes with Node, but the package platform is `{}`",
                        paint::error_prefix(),
                        platform.name()
                    );
                    ExitCode::FAILURE
                }
            }
            Project::Workspace {
                root,
                members,
                default_entry,
                ..
            } => run_workspace(&root, &members, args, entry, &default_entry),
            Project::Library { name, .. } => not_buildable_library(&name).into(),
        }
    })
}

// --- `--watch` mode (roadmap P5) --------------------------------------------

/// How often the watcher polls for changes.
const WATCH_POLL_INTERVAL: Duration = Duration::from_millis(300);

/// How often `VILAN_WATCH_LOG`'s trace says the loop is alive and found nothing
/// (tracker B208). Not a poll rate: the loop still polls every
/// [`WATCH_POLL_INTERVAL`], and every poll that finds a DIFFERENCE is traced
/// whenever it happens. This only rate-limits the silence, so a 300 s wait
/// leaves ~30 heartbeat lines instead of ~1000 — enough to separate "the loop
/// was polling and the file never moved" from "the loop was not polling".
const WATCH_LOG_HEARTBEAT: Duration = Duration::from_secs(10);

/// The build inputs a round declared or read beyond its `.vl` sources — the
/// **recorded-inputs** set the watcher polls alongside [`scan_vl`]. Two
/// producers feed it, and they are the whole list:
///
/// * `const asset::read`, recorded per compile by [`record_const_inputs`] —
///   misses included, because a file that was not there is still a dependency
///   whose APPEARANCE must trigger a round exactly as a change to it would.
/// * a manifest's `[[build.hook]]` `inputs`, recorded per hook run by
///   [`BuildHooks::record_watched_inputs`] — the declaration the freshness
///   stamp already reads, now read by the watcher too (G10).
///
/// Process-global because the recording compile runs several opaque call
/// frames below the watch loop's action closure. Accumulative across rounds: a
/// path a later round no longer reads stays watched, which costs at most one
/// round whose legs then verify by content and skip.
///
/// Declared **outputs** are deliberately absent. Only inputs are watched, so a
/// hook writing what it said it writes can never wake the loop that ran it —
/// the `.vl` scan's own invariant (`watch-mode.md`: a build can never trigger
/// its own rebuild) carried into this set.
///
/// What that invariant does not cover — here, and equally for `asset::read`
/// since the day those were recorded — is an input some OTHER unconditional
/// step rewrites every round. The poll compares modification times, exactly as
/// it does for `.vl` sources (saving a source with identical bytes is a round
/// today too), so a rewrite settles nothing by being byte-identical. The hook
/// does settle: its stamp is content-based and finds nothing moved, so it is
/// skipped. The ROUNDS stop when the step doing the rewriting stops.
///
/// Both locks below RECOVER from poisoning (backlog E97, the tree's one
/// posture). This changes nothing about the CLI's *panic* stance — a compiler
/// panic in a one-shot `vilan build` is still loud and fatal (AGENTS.md's fence
/// note) — but `--watch` is a long-lived loop, and a watch set that could stop
/// being readable would leave the loop silently blind to every recorded input.
/// An unwind can only ever leave this set holding a subset of one round's
/// paths, which the next round re-records.
static RECORDED_INPUT_PATHS: std::sync::Mutex<BTreeMap<PathBuf, InputReading>> =
    std::sync::Mutex::new(BTreeMap::new());

/// What a recorded input MEANS when it names a directory (G21, audit run 6's
/// F23) — the reading its recorder keys on, carried on the row the way G16 put
/// the entry KIND on the freshness stamp's rows, and for the same reason: two
/// consumers of one path have to agree about what the path says.
///
/// The two producers do not agree, because their gates do not. A
/// `[[build.hook]]`'s declared directory is digested as its WHOLE TREE
/// ([`file_digest`]), so an edit anywhere inside it re-runs the hook and must
/// wake a round. The const channel's listing verbs key a directory on its
/// IMMEDIATE membership and nothing else ([`vilan_core::const_eval::directory_input_hash`]
/// — the listing reads names, never bytes, and a file whose contents matter is
/// tracked in its own right by the `read`/`bundle`/`digest` that touched it, and
/// a nested directory by the `read_dir_all` that walked it). Recording both as
/// trees woke rounds the const key provably could not have moved for: editing a
/// listed file's content, or any file deeper in the tree. Harmless — the leg
/// then re-keys the directory, finds it unchanged and skips — but a round that
/// reliably does nothing is the watcher reading a declaration differently from
/// the gate it wakes.
///
/// Ordered so that [`InputReading::Tree`] wins when one path is recorded under
/// both: the tree is the superset, and the safe direction here is the spurious
/// round, never the missed one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum InputReading {
    /// The const channel's listing (`asset::read_dir`, `asset::read_dir_all`):
    /// a directory contributes its own entry alone.
    Listing,
    /// A `[[build.hook]]` `inputs` declaration: a directory contributes its own
    /// entry plus one per member of its tree.
    Tree,
}

/// Adds `inputs` to the watcher's recorded-input set — the one door into
/// [`RECORDED_INPUT_PATHS`], so every producer records the same way, and now
/// says which way that is.
fn record_watched_inputs(inputs: impl IntoIterator<Item = (PathBuf, InputReading)>) {
    let mut inputs = inputs.into_iter().peekable();
    if inputs.peek().is_none() {
        return;
    }
    let mut recorded = RECORDED_INPUT_PATHS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (path, reading) in inputs {
        let entry = recorded.entry(path).or_insert(reading);
        *entry = (*entry).max(reading);
    }
}

/// Records a compile's `const asset::…` inputs for the watcher, under the
/// channel's own reading of a directory: its immediate membership.
fn record_const_inputs(inputs: &[(PathBuf, Option<u64>)]) {
    record_watched_inputs(
        inputs
            .iter()
            .map(|(path, _)| (path.clone(), InputReading::Listing)),
    );
}

/// [`scan_vl`] plus the recorded build inputs: the full watched set. A recorded
/// path that does not exist stays out of the map — its later appearance inserts
/// an entry, which is exactly the snapshot difference that fires a round.
fn watch_snapshot(roots: &[PathBuf]) -> BTreeMap<PathBuf, SystemTime> {
    let mut files = scan_vl(roots);
    for (path, reading) in RECORDED_INPUT_PATHS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
    {
        insert_watched_input(path, *reading, &mut files);
    }
    files
}

/// Adds one recorded input to the snapshot: its modification time, and — when
/// it is a **directory** the recorder reads as a [`InputReading::Tree`] — one
/// entry per member of that tree.
///
/// Every recorded path gets its OWN entry whatever the reading, and that half is
/// N30: a directory contributed only its FILES, so a recorded-missing directory
/// that APPEARS EMPTY added no entry at all and started no round — while the
/// compile that failed on it (`asset::read_dir` records the miss) would now
/// succeed against the empty listing. The first file created inside it fired;
/// the appearance itself did not. The directory's value is its own mtime rather
/// than a rendering of its membership, because mtime is the instrument every
/// other entry in this map already uses, and a directory's mtime moves precisely
/// when a direct entry is added or removed — which is also exactly what the
/// const channel's listing key is a function of.
///
/// The TREE half belongs only to the declaration that reads a directory as one:
/// a `[[build.hook]]`'s `inputs`, which [`file_digest`] digests as the whole
/// tree, so an edit anywhere inside it re-runs the hook and must therefore wake
/// a round. Giving the const channel's directories the same expansion was G21:
/// it woke rounds for edits its gate keys nothing on. The reading now travels
/// with the row ([`InputReading`]), so one declaration is read one way by the
/// gate and by the watcher — the alignment G16 made for the stamp, on the other
/// producer.
///
/// The top-level path is resolved through a symlink (`fs::metadata`), matching
/// both the `asset::read` inputs this set has always carried and the stamp,
/// which resolves its own declared path the same way. That last half was only
/// half true until G15 — the stamp stat'd the link itself, so a declared link
/// to a DIRECTORY was watched here and unreadable there — and the comment
/// claiming the match is what let it stand. Inside a tree, a symlink is never
/// followed: the stamp digests the link's own target path there, and following
/// one could walk out of the tree or into a cycle.
fn insert_watched_input(
    path: &Path,
    reading: InputReading,
    files: &mut BTreeMap<PathBuf, SystemTime>,
) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if let Ok(modified) = metadata.modified() {
        files.insert(path.to_path_buf(), modified);
    }
    if metadata.is_dir() && reading == InputReading::Tree {
        collect_input_tree(path, files);
    }
}

/// Every path under a declared directory input → its modification time: each
/// file, and each nested DIRECTORY in its own right, for the same reason the
/// root gets an entry — a subdirectory appearing empty is a change to the tree
/// that no file entry can express. The stamp reads it that way too: a directory
/// is a row of its own in [`collect_tree`]. It was not until G16, and the
/// disagreement was visible exactly here — this entry woke a round that the
/// stamp then answered `Fresh`. An entry that cannot be read is skipped rather
/// than failing the snapshot: the watcher's job is to keep polling, and a path
/// it cannot stat is one whose later readability is itself a difference.
fn collect_input_tree(root: &Path, files: &mut BTreeMap<PathBuf, SystemTime>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if let Ok(modified) = metadata.modified() {
            files.insert(path.clone(), modified);
        }
        if metadata.is_dir() {
            collect_input_tree(&path, files);
        }
    }
}

/// Whether one command run — one `--watch` round — succeeded.
///
/// [`ExitCode`] is write-only: it is built from a verdict and handed to the
/// process, and nothing can read the verdict back out of it. A caller that only
/// forwards a code is well served by that; [`watch_loop`] is not, because it has
/// to *act* on a failed round — it restores the round's snapshot difference and
/// retries once (G14). So the three commands a watch session drives (`build`,
/// `check`, `test`) report a verdict that can be read, and [`run_or_watch`]
/// flattens it into an `ExitCode` at the one place that needs one.
///
/// Nothing is lost by the conversion. Every fallible step under those three
/// answers with `Err(ExitCode::FAILURE)` — the only failure code the CLI has —
/// so an arm below that reads such an `Err` as `Failed` and drops the code drops
/// nothing the `Err` had not already said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoundOutcome {
    Succeeded,
    Failed,
}

impl From<RoundOutcome> for ExitCode {
    fn from(outcome: RoundOutcome) -> ExitCode {
        match outcome {
            RoundOutcome::Succeeded => ExitCode::SUCCESS,
            RoundOutcome::Failed => ExitCode::FAILURE,
        }
    }
}

/// The failure value of a command's result currency, so one reporter serves
/// both: [`ExitCode`] for a command that answers straight to the process (a
/// `run` propagates the *program's* own code, any value, not just 0/1), and
/// [`RoundOutcome`] for one a `--watch` session drives and has to read back.
trait CommandFailure {
    fn failed() -> Self;
}

impl CommandFailure for ExitCode {
    fn failed() -> ExitCode {
        ExitCode::FAILURE
    }
}

impl CommandFailure for RoundOutcome {
    fn failed() -> RoundOutcome {
        RoundOutcome::Failed
    }
}

/// Runs `action` once and returns its exit code (no `--watch`, `roots` is `None`),
/// or — under `--watch` — re-runs it on every change to a `.vl` file under `roots`.
fn run_or_watch(roots: Option<Vec<PathBuf>>, mut action: impl FnMut() -> RoundOutcome) -> ExitCode {
    match roots {
        None => action().into(),
        Some(roots) => watch_loop(&roots, action),
    }
}

/// The directories to watch, from a command's path argument: an explicit directory
/// as-is (a workspace root covers every member); a file's parent (its `pkg::`
/// siblings); with no path, the nearest project root, else the working directory.
fn watch_roots(file: &Option<PathBuf>) -> Vec<PathBuf> {
    let root = match file {
        Some(path) if path.is_dir() => path.clone(),
        Some(path) => pkg_root_of(path),
        None => env::current_dir()
            .ok()
            .map(|cwd| find_project_root(&cwd).unwrap_or(cwd))
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    vec![root]
}

/// A snapshot of every `.vl` file under `roots` (recursively) → its last-modified
/// time. Only `.vl` files are tracked, so the compiler's own `.js` / `dist` / `.out`
/// output can never trigger a rebuild; comparing two snapshots detects edits,
/// additions, and removals.
fn scan_vl(roots: &[PathBuf]) -> BTreeMap<PathBuf, SystemTime> {
    let mut files = Vec::new();
    for root in roots {
        // A link out of the project is not this session's to watch either, and
        // saying so 3 times a second is not honesty. `fmt` carries the note.
        let _outside = collect_vl_files(root, &mut files);
    }
    files
        .into_iter()
        .filter_map(|path| {
            let modified = fs::metadata(&path).and_then(|meta| meta.modified()).ok()?;
            Some((path, modified))
        })
        .collect()
}

/// The exit code a watch session reports when `Ctrl-C` ends it: the shell
/// convention for "terminated by SIGINT" (128 + 2), used on every platform so
/// the two legs agree.
const WATCH_INTERRUPT_EXIT_CODE: i32 = 130;

/// Installs the `Ctrl-C` exit hook for a watch session.
///
/// [`watch_loop`] never returns from its loop, so without a hook the session's
/// temp script (`vilan-watch-<pid>.mjs`) outlives it — one leaked file per watch
/// session (`windows-support.md` §6; the per-round delete in [`run_watch`]
/// covers only restarts). The hook is deliberately tiny: remove the script,
/// exit. Removal is silent here, unlike the per-round one — the process is on
/// its way out and a warning would race the child's own shutdown.
///
/// It does not touch the child, and must not: on unix the terminal delivers
/// `SIGINT` to the whole process group, so the `node` child is already stopping
/// on its own; on Windows the child's kill-on-close Job object (see [`job`])
/// reaps its tree the moment this process exits. `ctrlc` runs the closure on
/// its own thread rather than inside the signal handler, which is what makes
/// filesystem work legal here at all — a raw `SIGINT` handler may not allocate
/// the path or run `atexit`.
fn install_watch_interrupt_hook() {
    // Best effort: a watch session is not worth failing over a handler the OS
    // (or a second install) refused.
    let _ = ctrlc::set_handler(|| {
        let _ = fs::remove_file(watch_script_path());
        std::process::exit(WATCH_INTERRUPT_EXIT_CODE);
    });
}

/// Runs `action`, then re-runs it whenever a watched `.vl` file changes — polling
/// every [`WATCH_POLL_INTERVAL`]. Returns only when there's nothing to watch;
/// otherwise loops until `Ctrl-C` — which stops any `run --watch` child (via the
/// shared terminal process group on unix, via the child's Job object on Windows)
/// and runs [`install_watch_interrupt_hook`]'s cleanup.
///
/// **A failed round keeps its change** (G14, ruled 2026-08-29). The difference
/// that woke a round is *consumed* only when the round succeeds: a round whose
/// action fails leaves the old snapshot in place, so the next poll still sees
/// that difference and re-fires the round — once. A second consecutive failure
/// consumes the difference and the session goes back to waiting for the next
/// change, which is the posture this loop has always had. Before, that posture
/// was the *only* one — `snapshot = next` ran before the action — so a round lost
/// to a transient failure (a `cmd` hiccup on a loaded runner, a hook racing
/// something outside the tree) was lost for good, and the session sat healthy and
/// silent until some unrelated file happened to move.
///
/// **Why the guard is once.** The other failure a round has is a *compile error*,
/// and for it "wait for the next change" is the right design: the fix is the next
/// edit, and a tree that cannot compile would retry forever. Retried once, a
/// broken tree compiles-fails a second time and then rests — one extra compile
/// per broken save, bounded, and the price of giving the transient case its round
/// back. Nothing here can tell the two apart (a failed command is a failed
/// command), which is exactly why the budget is one and not a policy.
///
/// **No hot loop.** A retry is an ordinary round: it rides
/// [`WATCH_POLL_INTERVAL`] like every other one, and the second failure consumes
/// the difference, so no sequence of failures produces a third automatic attempt.
/// A change landing *during* a pending retry is folded into that retry rather
/// than refreshing the budget — the conservative direction, and the one that
/// keeps "at most two runs per difference" true however edits are timed.
///
/// The retry is only as good as the verdict its caller hands back, and one
/// caller has none to give: see [`run_watch`], whose rounds report
/// [`RoundOutcome::Succeeded`] because their failures are already handled inside
/// the round.
fn watch_loop(roots: &[PathBuf], mut action: impl FnMut() -> RoundOutcome) -> ExitCode {
    if roots.iter().all(|root| !root.exists()) {
        eprintln!("{} nothing to watch (no such path)", paint::error_prefix());
        return ExitCode::FAILURE;
    }
    install_watch_interrupt_hook();
    let watched = roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    // The line names BOTH halves of the set. It used to say `.vl` alone, which
    // was already an understatement (recorded `asset::read` inputs have been
    // polled beside the scan for as long as they have been recorded) and became
    // a misleading one once hook `inputs` joined them: G10 was diagnosed off
    // this banner — "it says `.vl`, so the watcher is doing what it says and the
    // stamp must be wrong" — when the truth was the reverse. Nothing here can
    // enumerate the recorded set (the first round is what discovers it), so the
    // line states the rule instead of the contents.
    eprintln!(
        "{}",
        paint::err(
            paint::Style::CYAN,
            &format!(
                "[watch] watching {watched} for `.vl` changes and declared build \
                 inputs (Ctrl-C to stop)"
            )
        )
    );
    // The baseline snapshot is taken BEFORE the first action, never after: the
    // initial build can run for seconds, and a save landing inside it must
    // trigger a round, not vanish into the baseline. With the old order (build,
    // then snapshot) an edit made between the build's output appearing and the
    // snapshot being taken was baked in and silently never detected — E20's
    // four deadline-exhausted strikes across three environments were exactly
    // this window, widened by suite load; a human saving during the initial
    // build hit the same swallowed edit. Cost of this order: an edit during
    // the initial build causes one extra round — which is the correct behavior.
    let started = SystemTime::now();
    let mut snapshot = watch_snapshot(roots);
    watch_log::session_start(roots, snapshot.len());
    // The first round has no difference to keep — the baseline below is built
    // after it either way — so its verdict is nothing this loop can act on.
    watch_log::line("round 1 (the initial build) start");
    let round_started = Instant::now();
    let first = action();
    watch_log::line(&format!(
        "round 1 end verdict={first:?} in {:.3}s",
        round_started.elapsed().as_secs_f64()
    ));
    // The first build just revealed which `asset::read` inputs exist — paths
    // the baseline could not contain. Seed them in with E20's rule intact: an
    // input whose mtime predates the build joins the baseline (no spurious
    // round), one modified at or after the build's start does NOT, so the
    // next poll sees it and fires the round that re-reads it.
    for (path, modified) in watch_snapshot(roots) {
        if snapshot.contains_key(&path) {
            continue;
        }
        let seeded = modified < started;
        watch_log::seed(&path, modified, started, seeded);
        if seeded {
            snapshot.insert(path, modified);
        }
    }
    // Whether the round about to run is the ONE retry of a round that just
    // failed. This flag is the spin guard: the difference is kept for exactly
    // one re-fire, and the failure that ends a retry consumes it.
    let mut retrying = false;
    // Round 1 is counted, so the trace's round numbers match the session's.
    let mut round = 1_u64;
    // The trace's heartbeat: the polls that found nothing are the ones that
    // prove the loop is alive, and one line per 300 ms poll would bury the
    // ones that found something. One line per `WATCH_LOG_HEARTBEAT` says both.
    let mut last_heartbeat = Instant::now();
    loop {
        std::thread::sleep(WATCH_POLL_INTERVAL);
        let next = watch_snapshot(roots);
        if next == snapshot {
            if watch_log::enabled() && last_heartbeat.elapsed() >= WATCH_LOG_HEARTBEAT {
                last_heartbeat = Instant::now();
                watch_log::line(&format!(
                    "poll: no difference ({} entries watched, retrying={retrying})",
                    next.len()
                ));
            }
            continue;
        }
        if watch_log::enabled() {
            last_heartbeat = Instant::now();
            watch_log::line(&format!(
                "poll: {}",
                watch_log::snapshot_diff(&snapshot, &next)
            ));
        }
        round += 1;
        watch_log::line(&format!("round {round} start (retry={retrying})"));
        let round_started = Instant::now();
        eprintln!(
            "\n{}",
            paint::err(
                paint::Style::CYAN,
                if retrying {
                    "[watch] retrying the failed round"
                } else {
                    "[watch] change detected, re-running"
                }
            )
        );
        let outcome = action();
        watch_log::line(&format!(
            "round {round} end verdict={outcome:?} in {:.3}s",
            round_started.elapsed().as_secs_f64()
        ));
        match outcome {
            // Consumed by a round that dealt with it. `next` was read BEFORE the
            // action, so an edit landing while the round ran is still a
            // difference at the next poll — E20's rule, unchanged.
            RoundOutcome::Succeeded => {
                snapshot = next;
                retrying = false;
            }
            // NOT consumed: the same difference is still there at the next poll,
            // which is how the retry fires without a timer of its own.
            RoundOutcome::Failed if !retrying => {
                retrying = true;
                eprintln!(
                    "{}",
                    paint::err(
                        paint::Style::CYAN,
                        "[watch] the round failed; retrying it once on the next poll"
                    )
                );
            }
            // The retry failed too. Consume the difference and wait for the next
            // change: twice in a row is a broken tree rather than a hiccup, and
            // the fix for a broken tree is the next edit.
            RoundOutcome::Failed => {
                snapshot = next;
                retrying = false;
                eprintln!(
                    "{}",
                    paint::err(
                        paint::Style::CYAN,
                        "[watch] the retry failed too; waiting for the next change"
                    )
                );
            }
        }
    }
}

/// `vilan run --watch`: rebuild and restart the program on every change. Each round
/// stops the previous process first (so a server frees its port), then spawns the
/// new one without waiting and holds its handle for the next round.
///
/// When the project is a workspace with a browser leg and `--no-hmr` isn't set,
/// hot module replacement is active (hmr.md §1): a dev channel serves the browser,
/// and each round classifies the rebuilt bytes (hmr.md §6) — restarting the Node
/// child only when the server bundle changed, and pushing `swap` / `css` / `error`
/// to the browser instead of bouncing it. Otherwise this is the plain
/// restart-the-server loop, byte-for-byte as before.
fn run_watch(
    file: Option<PathBuf>,
    args: Vec<String>,
    no_hmr: bool,
    hmr_port: u16,
    entry: Option<String>,
) -> ExitCode {
    let roots = watch_roots(&file);
    let mut child: Option<ManagedChild> = None;
    let channel = if no_hmr {
        None
    } else {
        activate_hmr(&file, hmr_port)
    };
    let mut state = WatchState::default();
    let code = watch_loop(&roots, move || {
        match &channel {
            Some(channel) => {
                child = hmr_round(
                    channel,
                    file.clone(),
                    &args,
                    &mut state,
                    child.take(),
                    entry.as_deref(),
                );
            }
            None => {
                // The plain restart loop recompiles and respawns wholesale, so
                // the per-leg skip doesn't drop in naturally here (there are no
                // retained per-leg artifacts to reuse) (backlog E12).
                if let Some(mut previous) = child.take() {
                    let _ = previous.kill();
                    let _ = previous.wait();
                    // The child is reaped, so nothing holds the round's temp
                    // script any more and it can be removed before the next one
                    // writes it.
                    remove_watch_script();
                }
                child = build_and_spawn_run(file.clone(), &args, entry.as_deref());
            }
        }
        // **This path has no failure verdict to give**, so it reports none, and
        // [`watch_loop`]'s retry does not reach it (G14's determination, made
        // here at the code):
        //
        // * Neither round *returns* one. `hmr_round` and `build_and_spawn_run`
        //   both answer with an `Option<ManagedChild>`, and `None` there means
        //   "no Node child to hold" — which a browser-only workspace produces on
        //   a perfectly good round — while a failed HMR round returns the
        //   PREVIOUS child, alive. The value cannot separate the two, and
        //   reading a failure out of it would be inventing one.
        // * The failure is already handled where it happens: a failed round
        //   reports to the terminal, pushes `error` to the browser overlay, and
        //   keeps the last good build running. `WatchState::failed` records that
        //   for the classifier's benefit — recompile every leg next round — and
        //   is not a verdict about the round.
        // * The failure it would carry is the one the ruling deliberately does
        //   not retry. On the dev loop the overwhelming failure is a compile
        //   error, whose fix is the next edit; retrying it here would cost a
        //   second full recompile of every leg and a second error overlay per
        //   broken save.
        //
        // The transient case G14 is about — a hook hiccup — is legible on the
        // `build` / `check` / `test` rounds, which is where the retry lives.
        RoundOutcome::Succeeded
    });
    // Reached ONLY when there was nothing to watch — i.e. before any script was
    // written. The other two cleanup paths are the per-round delete above (which
    // closes the Windows sharing violation: the script is never rewritten while
    // a `node` child holds it) and `install_watch_interrupt_hook`, which covers
    // the way a watch session actually ends — `Ctrl-C`, from inside a loop that
    // never returns.
    remove_watch_script();
    code
}

/// The carried-over state of an HMR `run --watch` across rounds (backlog E12):
/// the previous good artifacts (with their source sets) for the byte classifier
/// and the per-leg skip, plus the two guards that force a full recompile
/// regardless of the changed set.
#[derive(Default)]
struct WatchState {
    /// Each host leg's last good artifact — the classifier's `previous`, and the
    /// source of the reused bytes when a leg is skipped.
    legs: Vec<hmr::LegArtifact>,
    /// The previous round failed to compile: no leg has a trustworthy artifact
    /// to reuse, so recompile every leg until a round succeeds.
    failed: bool,
    /// A fingerprint of every `vilan.toml` under the watch root (workspace,
    /// members, and in-tree dependencies alike). A manifest can change a leg's
    /// output without touching its `.vl` sources (a dependency, a platform, a
    /// build option), so a change here forces a full recompile. `None` until
    /// the first round establishes it.
    manifest: Option<u64>,
}

/// Turns HMR on for `run --watch` when the project is a workspace with at least
/// one browser leg (hmr.md §1). Binds the dev channel on `127.0.0.1:port`
/// (`port` `0` ⇒ ephemeral) and announces it. A port already in use is a warning,
/// not a crash — the watch continues without HMR. `None` (silently) when the
/// project isn't HMR-eligible.
fn activate_hmr(file: &Option<PathBuf>, port: u16) -> Option<hmr::DevChannel> {
    let project = resolve_project(file.clone()).ok()?;
    let Project::Workspace { root, members, .. } = &project else {
        return None;
    };
    if !members
        .iter()
        .any(|(_, platform)| matches!(platform, Platform::Browser))
    {
        return None;
    }
    match hmr::DevChannel::bind(port, root.join("dist")) {
        Ok(channel) => {
            println!(
                "{}",
                paint::out(
                    paint::Style::CYAN,
                    &format!("hmr: dev channel on 127.0.0.1:{}", channel.port())
                )
            );
            Some(channel)
        }
        Err(error) => {
            eprintln!(
                "{} HMR dev channel could not bind 127.0.0.1:{port} ({error}); \
                 continuing to watch without HMR",
                paint::warning_prefix()
            );
            None
        }
    }
}

/// One HMR watch round (hmr.md §6): rebuild every host leg, classify the raw
/// bundle bytes against the previous round, write `dist/` (browser legs get the
/// shim prepended, with this build's version embedded), restart the Node child
/// only when the server bundle changed, and push the round event to the browser.
/// A compile failure pushes an `error` event and keeps the last good build —
/// the standard HMR contract — leaving `previous` and the running `child` intact.
fn hmr_round(
    channel: &hmr::DevChannel,
    file: Option<PathBuf>,
    args: &[String],
    state: &mut WatchState,
    child: Option<ManagedChild>,
    entry: Option<&str>,
) -> Option<ManagedChild> {
    let (root, members, default_entry, hooks) = match resolve_project(file) {
        Ok(Project::Workspace {
            root,
            members,
            default_entry,
            hooks,
        }) => (root, members, default_entry, hooks),
        // The project stopped being an HMR-eligible workspace (a manifest edit,
        // say). Report it as a failed round: overlay + keep the last good build.
        Ok(_) | Err(_) => {
            eprintln!(
                "{} the HMR project is no longer a runnable workspace",
                paint::error_prefix()
            );
            state.failed = true;
            channel.push("error", Some("build failed; see the terminal"));
            return child;
        }
    };

    // The `[build] run` hooks run once per round, before this round's compile
    // (A9) — a Tailwind bridge regenerates its CSS from the sources that just
    // changed. A failing hook fails the round like a failing compile: report,
    // overlay, keep the last good build.
    if let Err(message) = hooks.run(false) {
        eprintln!("{} {message}", paint::error_prefix());
        state.failed = true;
        channel.push("error", Some("build failed; see the terminal"));
        return child;
    }

    // Decide which legs this round may SKIP — reuse the previous artifact for
    // rather than recompile (backlog E12, half b). Reuse is decided by CONTENT,
    // never by mtime: a leg qualifies only when every source its artifact was
    // compiled from re-hashes, right now, to the hash it was compiled with
    // (mtime merely *triggers* rounds — review finding, 2026-07-21). The safe
    // default (skip nothing) covers the first round, a prior failure, and a
    // manifest change; a deleted or unreadable source fails its re-hash and
    // recompiles by construction.
    let manifest = manifest_fingerprint(&root);
    let manifest_changed = state.manifest.is_some_and(|previous| previous != manifest);
    state.manifest = Some(manifest);
    let force_full = hmr::round_forces_full(state.legs.is_empty(), state.failed, manifest_changed);
    let current_hash = |path: &Path| -> Option<u64> {
        // A recorded input that is a DIRECTORY re-hashes as its listing
        // (`asset::read_dir` / `read_dir_all`, const-eval.md §3.1). Without
        // this arm the read below fails on it and the leg could never skip;
        // with it, an unchanged directory compares equal and a file appearing
        // or vanishing anywhere in a listed tree fails the compare, which is
        // exactly the invalidation the tracked-directory doctrine promises.
        if path.is_dir() {
            return vilan_core::const_eval::directory_input_hash(path);
        }
        // Read the same way the compiler reads (BOM dropped,
        // windows-support.md §2), or the hash recorded from the text it
        // consumed could never match.
        vilan_core::util::read_source(path)
            .ok()
            .map(|text| vilan_core::content_hash(&text))
    };
    // B203 — the legs in artifact-dependency order, and the edges the skip
    // decision consults at each leg's own turn. The HMR round writes `dist/`
    // AFTER the whole compile loop (the shim carries a version the classifier
    // has not decided yet), so re-hashing against the disk can never see this
    // round's own work: here the edge itself is the instrument, and a leg that
    // reads a recompiling leg's artifact recompiles with it.
    let dist_directory = root.join("dist");
    let schedule = {
        let scheduled: Vec<ScheduledLeg> = members
            .iter()
            .map(|(unit, platform)| {
                let previous = state.legs.iter().find(|leg| leg.name == unit.name);
                ScheduledLeg {
                    name: &unit.name,
                    extension: platform.script_extension(),
                    bundled: previous.map(|leg| leg.bundled.as_slice()).unwrap_or(&[]),
                    sources: previous.map(|leg| &leg.sources),
                }
            })
            .collect();
        leg_schedule(&dist_directory, &scheduled)
    };
    // Walked in SCHEDULE order, so a leg's answer is given after every leg it
    // reads has already given one — which is what makes
    // `downstream_of_a_recompile` a statement about this round rather than a
    // guess about it. A leg with no record recompiles by construction, and is
    // recorded as recompiling so the legs downstream of it recompile too.
    let mut recompiled: BTreeSet<usize> = BTreeSet::new();
    let mut skip: BTreeSet<String> = BTreeSet::new();
    for index in &schedule.order {
        let (unit, platform) = &members[*index];
        if platform.is_none() {
            continue;
        }
        let reusable = !force_full
            && !schedule.downstream_of_a_recompile(*index, &recompiled)
            && state
                .legs
                .iter()
                .find(|leg| leg.name == unit.name)
                .is_some_and(|previous| hmr::leg_is_current(&previous.sources, current_hash));
        if reusable {
            skip.insert(unit.name.clone());
        } else {
            recompiled.insert(*index);
        }
    }

    // Compile every host leg (skipped legs excepted), capturing the RAW bundle
    // bytes (before the shim is prepended — the shim embeds the version, so
    // shim-inclusive bytes would differ every round and misclassify everything
    // as a swap).
    let mut next = Vec::new();
    let mut other_assets: Vec<(String, BTreeMap<String, String>)> = Vec::new();
    // B203's order: a leg whose artifact another leg reads compiles first, so
    // `next` — and therefore `dist/` — is written producer before consumer.
    for index in schedule.order.clone() {
        let (unit, platform) = &members[index];
        if platform.is_none() {
            continue;
        }
        if skip.contains(&unit.name) {
            // Reuse the previous artifact verbatim: the leg's sources are
            // unchanged, so a recompile would reproduce these exact bytes (the
            // classifier then sees no change and pushes nothing — identical to
            // having recompiled). Its non-css assets are already on disk from the
            // round that built them, so they need no rewrite.
            let prior = state
                .legs
                .iter()
                .find(|leg| leg.name == unit.name)
                .expect("skippable_legs only skips a leg with a previous artifact");
            println!(
                "{}",
                paint::out(
                    paint::Style::CYAN,
                    &format!("hmr: skipped {} (sources unchanged)", unit.name)
                )
            );
            next.push(prior.clone());
            continue;
        }
        note_split_ignored(unit);
        let mut overlay_text = String::new();
        let compiled = match compile_unit(
            unit,
            *platform,
            CompileGoal::Emit,
            false,
            matches!(platform, Platform::Browser),
            Some(&mut overlay_text),
            // Dev builds ignore `split` (`bundle-splitting.md` §4): HMR
            // classifies and swaps whole bundles. The leg's chunk namespace is
            // swept below, so a `vilan build` before this one leaves nothing
            // behind describing a split that is no longer on disk.
            None,
        ) {
            Ok(compiled) => compiled,
            // `compile_unit` has already reported the diagnostics to the
            // terminal (unchanged); `overlay_text` is the SAME diagnostics
            // rendered ANSI-free for the in-page overlay (hmr.md §§2/§6, the S1
            // residue closed). Keep the last good build.
            Err(_) => {
                state.failed = true;
                let message = if overlay_text.is_empty() {
                    "build failed; see the terminal"
                } else {
                    overlay_text.as_str()
                };
                channel.push("error", Some(message));
                return child;
            }
        };
        let mut assembled = vilan_core::const_eval::assemble_assets(&compiled.assets);
        let css = assembled
            .remove("css")
            .filter(|content| !content.is_empty());
        // Any non-css asset kind still lands on disk each round, exactly as
        // `write_assets` would put it (uniform with the build/run paths); it
        // just doesn't participate in classification — css is the only kind
        // the dev runtime knows how to hot-swap. Pushed even when EMPTY: a
        // compiled leg with no non-css kinds is how the write phase below
        // learns a kind stopped emitting and prunes its file (backlog G6) —
        // only a SKIPPED leg, whose files are still current, stays out.
        other_assets.push((unit.name.clone(), assembled));
        next.push(hmr::LegArtifact {
            name: unit.name.clone(),
            is_browser: matches!(platform, Platform::Browser),
            script_extension: platform.script_extension(),
            bundle: compiled.javascript,
            css,
            bundled: compiled.bundled,
            sources: compiled.sources.into_iter().collect(),
        });
    }

    // The ONE Node leg this watch runs (A15): `--entry` picks it in a multi-node
    // workspace, a lone node leg is picked automatically, a browser-only workspace
    // has none. The non-selected node legs compiled above (they are part of the
    // workspace) but are never launched, and — since they are not run and not
    // served — a change to one of them drives no restart (the classifier keys the
    // restart on the SELECTED leg only). An ambiguous choice is reported below,
    // when a restart is actually attempted.
    let selection = select_node_entry(&members, entry, &default_entry);
    let server_leg = match &selection {
        Ok(Some(unit)) => Some(unit.name.as_str()),
        _ => None,
    };
    let decision = hmr::classify(&state.legs, &next, server_leg);
    if decision.bump_version {
        channel.bump_version();
    }
    let version = channel.version();

    // Write `dist/` from the freshly-compiled legs: browser bundles carry the
    // shim (with the current port + version embedded) so every served browser
    // bundle's version matches what the channel reports on connect; node bundles
    // and CSS sidecars are written verbatim. The directory is the one the leg
    // schedule already named, so the round has ONE idea of where `dist/` is.
    let dist = dist_directory;
    if let Err(error) = fs::create_dir_all(&dist) {
        eprintln!(
            "{} cannot create {}: {error}",
            paint::error_prefix(),
            dist.display()
        );
        state.failed = true;
        channel.push("error", Some("build failed; see the terminal"));
        return child;
    }
    let reserved: Vec<LegNamespace> = next
        .iter()
        .map(|leg| LegNamespace {
            leg: leg.name.clone(),
            extension: leg.script_extension,
        })
        .collect();
    // Shared across the round's legs, for the reason `reserved` is: `dist/` is
    // one directory, so two legs bundling two different files to one name is a
    // collision this round must refuse rather than resolve by copy order.
    let mut bundled_names: BTreeMap<String, PathBuf> = BTreeMap::new();
    for leg in &next {
        let bundle_path = dist.join(format!("{}.{}", leg.name, leg.script_extension));
        let contents = if leg.is_browser {
            hmr::instrument(
                &leg.bundle,
                channel.port(),
                channel.token(),
                version,
                &leg.name,
            )
        } else {
            leg.bundle.clone()
        };
        if let Err(error) = fs::write(&bundle_path, contents) {
            eprintln!(
                "{} cannot write {}: {error}",
                paint::error_prefix(),
                bundle_path.display()
            );
        }
        if let Some(css) = &leg.css {
            let css_path = dist.join(format!("{}.css", leg.name));
            if let Err(error) = fs::write(&css_path, css) {
                eprintln!(
                    "{} cannot write {}: {error}",
                    paint::error_prefix(),
                    css_path.display()
                );
            }
        }
        // This round emitted the leg whole, so nothing of a previous split build
        // of it may remain (`bundle-splitting.md` §S3, item 4) — and a browser
        // leg still writes its build manifest, because the dev loop is exactly
        // where a server asks what its client leg emitted (`serve_build`'s watch
        // policy re-reads per request, but the DESCRIPTION is read once at boot,
        // and a watch round that swept it would leave the restarted server with
        // nothing to describe).
        // The leg's bundled resources ride every round, including a SKIPPED
        // one: `next` carries the previous round's artifact verbatim, so the
        // copy is idempotent and `dist/` never loses an asset to a round that
        // recompiled nothing. The copy reads the SOURCE, so a round TRIGGERED
        // by an edited resource carries the new bytes whether or not the leg
        // recompiled — which is what makes `asset_body`'s watch-mode re-read
        // see them (`dev-refresh.md` §5, item 1). The trigger itself is the
        // build-input record `record_const_inputs` hands the watcher; without
        // it no round fires and `dist/` keeps the first round's copy forever.
        let assets = write_bundled(
            &dist,
            &leg.bundled,
            &leg.name,
            &reserved,
            &mut bundled_names,
        )
        .unwrap_or_default();
        let styles = leg.css.as_ref().map(|_| format!("{}.css", leg.name));
        let _ = write_chunks(
            &bundle_path,
            &[],
            styles.as_deref(),
            &assets,
            leg.is_browser,
        );
    }
    for (name, kinds) in &other_assets {
        let flushed: BTreeSet<String> = kinds
            .keys()
            .filter(|kind| recordable_emit_kind(kind))
            .cloned()
            .collect();
        prune_and_record_asset_kinds(&dist, name, &flushed);
        for (kind, content) in kinds {
            let asset_path = asset_kind_path(&dist, name, kind);
            if let Err(error) = fs::write(&asset_path, content) {
                eprintln!(
                    "{} cannot write {}: {error}",
                    paint::error_prefix(),
                    asset_path.display()
                );
            }
        }
    }

    state.legs = next;
    // This round completed: clear the failure guard so the next round may skip
    // again (the previous-failure force-full no longer applies).
    state.failed = false;

    // Restart the Node child only when the server bundle changed (or on the
    // first round, to spawn it). A client-only or CSS-only round leaves the
    // server running and its port warm.
    let mut child = child;
    if decision.restart_server {
        if let Some(mut running) = child.take() {
            let _ = running.kill();
            let _ = running.wait();
        }
        child = match &selection {
            Ok(Some(unit)) => {
                // Run from the workspace root so the server reads sibling
                // `dist/` bundles, exactly as `run_workspace` /
                // `build_and_spawn_run`.
                let script = artifact_path(Path::new("dist"), &unit.name, NODE_LEG);
                // `dev-refresh.md` §5 item 2: the dev channel's port, so the
                // child's `std::watch::force_refresh()` (a no-op without it)
                // knows where to POST — and, since backlog E93, this run's token
                // beside it, because every route now requires one. The child is
                // the one legitimate caller that is not the browser shim, and an
                // inherited environment variable is how it already learns
                // everything else about the session (`VILAN_WATCHING`,
                // `VILAN_HMR_PORT`): nothing on disk, nothing to clean up, and
                // no way for another process to pick it up.
                let extra = [
                    ("VILAN_HMR_PORT", channel.port().to_string()),
                    ("VILAN_HMR_TOKEN", channel.token().to_string()),
                ];
                match spawn_watched_node(&script, args, Some(&root), &extra) {
                    Ok(spawned) => Some(spawned),
                    Err(error) => {
                        eprintln!("{} failed to launch `node`: {error}", paint::error_prefix());
                        None
                    }
                }
            }
            // No node leg at all: HMR still serves the browser leg(s).
            Ok(None) => None,
            // 2+ node legs and no `--entry` (or a bad `--entry`): report it (once,
            // on the first round's spawn attempt) and serve the browser anyway.
            Err(message) => {
                eprintln!("{} {message}", paint::error_prefix());
                None
            }
        };
    }

    match &decision.push {
        // The swap carries this round's COMPLETE browser stylesheet set. A swap
        // re-evaluates the bundle without reloading the document (hmr.md §3),
        // so without this nothing in the round refreshes the page's stylesheets
        // — a round that changed a bundle AND its sidecar dropped the sidecar,
        // and a sidecar that appeared or vanished had no way to reach the page
        // at all (kolt.local 007). `state.legs` is this round's legs (assigned
        // above), so an absent name is the round's own statement that it emits
        // no stylesheet for that leg.
        Some(hmr::Push::Swap) => {
            let sheets: Vec<String> = state
                .legs
                .iter()
                .filter(|leg| leg.is_browser && leg.css.is_some())
                .map(|leg| format!("{}.css", leg.name))
                .collect();
            channel.push_swap(&sheets);
        }
        Some(hmr::Push::Css(assets)) => {
            for asset in assets {
                channel.push_css(asset);
            }
        }
        None => {}
    }

    child
}

/// A fingerprint of the workspace + member `vilan.toml` files (backlog E12). A
/// change here — a dependency, a platform, a build option — can alter a leg's
/// output without touching any `.vl` source, so a differing fingerprint forces
/// the round to recompile every leg rather than skip. Walks the watch root for
/// **every** `vilan.toml` (workspace, members, and in-tree dependency packages
/// alike — a dependency's manifest changes its dependents' output too) and
/// hashes each path + content (unreadable ⇒ `None`), so an added, removed, or
/// edited manifest all shift the value.
///
/// The walk is [`TreeWalk`]'s, the same one `.vl` collection uses (G18): it
/// followed directory links unguarded too, so a cycle hung the watch round
/// before it started and a link out of the project put a stranger's manifests
/// into this project's fingerprint.
fn manifest_fingerprint(root: &Path) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut manifests = Vec::new();
    let mut walk = TreeWalk::rooted_at(root);
    walk.walk(root, &mut |path| {
        if path.file_name().and_then(|name| name.to_str()) == Some("vilan.toml") {
            manifests.push(path.to_path_buf());
        }
    });
    manifests.sort();
    let mut hasher = DefaultHasher::new();
    for path in manifests {
        path.hash(&mut hasher);
        fs::read(&path).ok().hash(&mut hasher);
    }
    hasher.finish()
}

/// The temp script a single-package `run --watch` round executes. One per
/// process (the pid keys it), rewritten each round.
fn watch_script_path() -> PathBuf {
    env::temp_dir().join(format!("vilan-watch-{}.mjs", std::process::id()))
}

/// Removes the round's temp script, best effort. Called once the child that was
/// executing it is killed AND reaped: Windows has no unlink-while-open, so
/// rewriting the file under a live `node` is an intermittent sharing violation
/// — and leaving it behind is a temp-directory leak on every platform
/// (`windows-support.md` §5). A missing file is success.
fn remove_watch_script() {
    let script = watch_script_path();
    if let Err(error) = fs::remove_file(&script)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!(
            "{} cannot remove {}: {error}",
            paint::warning_prefix(),
            script.display()
        );
    }
}

/// Builds the run target and spawns it with Node **without waiting**, returning the
/// child so the next `run --watch` round can stop it. `None` after reporting a
/// compile error or a non-runnable project.
fn build_and_spawn_run(
    file: Option<PathBuf>,
    args: &[String],
    entry: Option<&str>,
) -> Option<ManagedChild> {
    let project = match resolve_project(file) {
        Ok(project) => project,
        Err(message) => {
            eprintln!("{} {message}", paint::error_prefix());
            return None;
        }
    };
    // The plain (non-HMR) watch round builds too, so its hooks run first (A9).
    if run_build_hooks(&project, false).is_err() {
        return None;
    }
    let launch =
        |script: &Path, cwd: Option<&Path>| match spawn_watched_node(script, args, cwd, &[]) {
            Ok(child) => Some(child),
            Err(error) => {
                eprintln!("{} failed to launch `node`: {error}", paint::error_prefix());
                None
            }
        };
    match project {
        Project::Single { unit, platform, .. } => {
            let platform = platform.unwrap_or_default();
            if !matches!(platform, Platform::Node { .. }) {
                eprintln!(
                    "{} `vilan run` executes with Node, but the package platform is `{}`",
                    paint::error_prefix(),
                    platform.name()
                );
                return None;
            }
            let compiled = compile_unit(
                &unit,
                Platform::default(),
                CompileGoal::Emit,
                false,
                false,
                None,
                None,
            )
            .ok()?;
            // Assets go beside the *canonical* build output — `<entry>.css`, where
            // `build` writes them and the served program reads them — not beside the
            // /tmp watch script Node executes (which nothing serves). Each watch
            // round thus refreshes the on-disk sidecar for the dev loop (hmr.md §11
            // S0); the workspace arm below gets this for free via
            // `build_workspace_artifacts`.
            let output_path = unit.entry.with_extension(platform.script_extension());
            write_assets(&output_path, &compiled.assets);
            // Bundled resources land beside that same canonical output, so a
            // watched lone package serves what this round produced — and are
            // recorded under the same leg name `write_assets` just used, so a
            // resource this round stopped naming leaves with it.
            write_bundled(
                output_path.parent().unwrap_or(Path::new(".")),
                &compiled.bundled,
                &leg_name(&output_path),
                &[],
                &mut BTreeMap::new(),
            )
            .ok()?;
            let script = watch_script_path();
            if let Err(error) = fs::write(&script, compiled.javascript) {
                eprintln!(
                    "{} cannot write {}: {error}",
                    paint::error_prefix(),
                    script.display()
                );
                return None;
            }
            launch(&script, None)
        }
        Project::Workspace {
            root,
            members,
            default_entry,
            ..
        } => {
            let server = match select_node_entry(&members, entry, &default_entry) {
                Ok(Some(unit)) => unit,
                Ok(None) => {
                    eprintln!(
                        "{} no `node` package in this workspace to run",
                        paint::error_prefix()
                    );
                    return None;
                }
                Err(message) => {
                    eprintln!("{} {message}", paint::error_prefix());
                    return None;
                }
            };
            if build_workspace_artifacts(&root, &members, false, Emission::WholeBundles, None)
                .is_err()
            {
                return None;
            }
            launch(
                &artifact_path(Path::new("dist"), &server.name, NODE_LEG),
                Some(&root),
            )
        }
        Project::Library { name, .. } => {
            not_buildable_library(&name);
            None
        }
    }
}

/// Prints an `error: <message>` line and answers with the caller's own failure
/// value: an [`ExitCode`] for a command that reports straight to the process, a
/// [`RoundOutcome`] for one a `--watch` session drives and must read.
fn report_error<T: CommandFailure>(message: &str) -> T {
    eprintln!("{} {message}", paint::error_prefix());
    T::failed()
}

/// Reports that a `none`-platform package can't be built (it's a pure library).
fn no_host_platform() -> RoundOutcome {
    eprintln!(
        "{} the platform is `none` (a pure library); pick a host to build for with \
         `--platform node` or `--platform browser`",
        paint::error_prefix()
    );
    RoundOutcome::Failed
}

/// Reports that a `[library]` can't be built or run on its own — it's compiled only
/// as a dependency of an app.
fn not_buildable_library(name: &str) -> RoundOutcome {
    eprintln!(
        "{} `{name}` is a `[library]`, built only as a dependency of an app, not on its own. \
         Verify its platform contract with `vilan check`, or build an app that depends on it.",
        paint::error_prefix()
    );
    RoundOutcome::Failed
}

/// Checks a standalone `[library]`: it has no fixed build platform, so instead of a
/// single-platform compile it verifies the **platform contract** (§4.2) — every
/// module's `pkg::` imports must resolve for every platform that module's layer
/// serves. Reports any violation; clean ⇒ success.
fn check_library(dir: &Path, name: &str) -> RoundOutcome {
    let spec = vilan_core::manifest::resolve_library(dir);
    let violations = check_library_contract(&spec);
    if violations.is_empty() {
        println!(
            "{name}: {}",
            paint::out(paint::Style::GREEN, "platform contract OK")
        );
        RoundOutcome::Succeeded
    } else {
        for violation in &violations {
            eprintln!("{} {}", paint::error_prefix(), violation.msg);
        }
        RoundOutcome::Failed
    }
}

/// The effective build platform: an explicit `--platform`/`--target` flag wins (it
/// may name any platform, including `none`); otherwise the package's declared
/// `target`; otherwise the `node` default. `Err` carries a descriptive message for
/// an unrecognized or unsupported flag value.
fn effective_platform(flag: Option<&str>, package: Option<Platform>) -> Result<Platform, String> {
    match flag {
        Some(name) => Platform::parse(name),
        None => Ok(package.unwrap_or_default()),
    }
}

/// Validates a `--backend` flag value (only `js` today). The returned [`Backend`]
/// selects nothing yet — there's a single backend — so this exists to reject an
/// unknown name (e.g. `wasm`, not yet implemented) at the CLI boundary rather than
/// silently ignoring it.
fn effective_backend(flag: Option<&str>) -> Result<Backend, String> {
    match flag {
        Some(name) => {
            Backend::parse(name).ok_or_else(|| format!("unknown backend `{name}` (expected `js`)"))
        }
        None => Ok(Backend::default()),
    }
}

/// A package's source root from a bare entry file (no manifest): the entry's
/// parent directory, where its `import pkg::..` siblings live. Empty (a bare
/// filename) means the working directory.
fn pkg_root_of(entry: &Path) -> PathBuf {
    entry
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

/// Checks the ENTRY file's on-disk spelling, the way module resolution checks a
/// module's (`windows-support.md` §5, ratified call (c); §12 recorded the entry
/// as the gap that scoping left open). `vilan build Main.vl` on NTFS opens
/// `main.vl` and builds; the same command on Linux does not — a program that
/// builds on one machine and not another, which is what the check exists to
/// stop, and nothing about it is specific to an `import`.
///
/// The split is the same one `case_exact_mismatch` takes for a module: the
/// package root, and the components the build configuration joined onto it — so
/// an `[entry.<name>] path = "web/client.vl"` has its DIRECTORY checked too.
/// When the two do not nest (a bare `vilan build Main.vl` has `pkg_root = "."`,
/// which `strip_prefix` cannot cancel) the entry's own parent is the root and
/// its file name the single component. Directories ABOVE the package root are
/// deliberately not checked: they are how this machine was invoked, not part of
/// the program.
fn entry_case_mismatch(entry: &Path, pkg_root: &Path) -> Option<(String, String)> {
    if let Ok(relative) = entry.strip_prefix(pkg_root)
        && !relative.as_os_str().is_empty()
    {
        return vilan_core::util::case_exact_mismatch(pkg_root, relative);
    }
    let name = entry.file_name()?;
    vilan_core::util::case_exact_mismatch(&pkg_root_of(entry), Path::new(name))
}

/// Formats every `.vl` file under `paths` (a file, a directory walked
/// recursively, or the working directory when empty). In `--check` mode it only
/// reports files that would change; otherwise it rewrites them in place. The
/// formatter leaves a file untouched when it's already formatted or contains a
/// construct it can't yet print (it never produces non-round-tripping output).
///
/// A file under a package's declared `generated` root is not formatted at all —
/// see [`exclude_generated`], which is applied to the collected set so `--check`
/// and the rewrite are one rule rather than two that have to be kept in step.
fn fmt(paths: &[PathBuf], check: bool) -> ExitCode {
    let roots: Vec<PathBuf> = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths.to_vec()
    };
    let mut files = Vec::new();
    // ONE walk across every root, so the identity set spans them: overlapping
    // roots (`vilan fmt --check src src/pkg`) name the same files twice on the
    // command line and must still format each of them once (B213).
    let outside: BTreeSet<PathBuf> = collect_vl_files_across(&roots, &mut files)
        .into_iter()
        .collect();
    report_links_outside_the_project(&outside);
    exclude_generated(&mut files);
    let mut changed = 0;
    let mut failed = false;
    for file in &files {
        let source = match fs::read_to_string(file) {
            Ok(source) => source,
            Err(error) => {
                eprintln!(
                    "{} cannot read {}: {error}",
                    paint::error_prefix(),
                    file.display()
                );
                failed = true;
                continue;
            }
        };
        let formatted = vilan_core::formatter::format(&source);
        if formatted == source {
            continue;
        }
        if check {
            println!(
                "{} {}",
                paint::out(paint::Style::YELLOW, "would reformat"),
                paint::out(paint::Style::BOLD, &file.display().to_string())
            );
            changed += 1;
        } else if let Err(error) = fs::write(file, &formatted) {
            eprintln!(
                "{} cannot write {}: {error}",
                paint::error_prefix(),
                file.display()
            );
            failed = true;
        } else {
            println!(
                "{} {}",
                paint::out(paint::Style::GREEN, "formatted"),
                paint::out(paint::Style::BOLD, &file.display().to_string())
            );
        }
    }
    if failed || (check && changed > 0) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Drops every file that lives under a declared `generated` root, and says so
/// once per root (`build-hooks.md` §12.4).
///
/// A package's products are not authored: not reviewed as diffs, not formatted
/// as source. Formatting one is worse than pointless — a `[[build.hook]]`
/// declares its generated module in `outputs`, freshness digests that file by
/// content, so the reformat re-stales the hook, the generator rewrites the file
/// unformatted, and the two undo each other on every round, forever (§12.1).
///
/// It happens **here**, after collection and in `fmt` alone, rather than inside
/// [`collect_vl_files`] — which `scan_vl` shares with the watcher, and the
/// watcher must keep seeing these files. A generated `.vl` that changed is a
/// source input that changed; a round that ignored it would compile stale bytes.
/// Formatting and rebuilding want opposite answers about the same file, and only
/// one of them is this rule's.
///
/// The exclusion is unconditional: naming the file on the command line does not
/// lift it. That is what lets the language server honor the same rule for
/// format-on-save, which reaches a file by its exact path and nothing else.
fn exclude_generated(files: &mut Vec<PathBuf>) {
    // Every file in one directory has the same ancestors, so the covering root
    // is a property of the directory — one manifest walk per directory rather
    // than one per file, which matters at the thousand-generated-modules scale
    // the key exists for.
    let mut covering: HashMap<PathBuf, Option<PathBuf>> = HashMap::new();
    let mut skipped: BTreeMap<PathBuf, usize> = BTreeMap::new();
    files.retain(|file| {
        let directory = file.parent().unwrap_or(Path::new(".")).to_path_buf();
        let root = covering
            .entry(directory)
            .or_insert_with_key(|directory| {
                vilan_core::manifest::generated_root_covering(directory)
            })
            .clone();
        match root {
            Some(root) => {
                *skipped.entry(root).or_default() += 1;
                false
            }
            None => true,
        }
    });
    // One dim line per root that actually skipped something — the honesty budget
    // `Fresh <name>` and the un-opted-in dependency note already spend, for the
    // same reason: a tool that quietly stops doing what it was asked is the
    // failure mode this design cannot afford. Never per file (a thousand
    // generated icons is a thousand lines nobody reads), never when the
    // exclusion excluded nothing, and never a warning — skipping a product is
    // the correct outcome, so the exit code does not move.
    for (root, count) in &skipped {
        let plural = if *count == 1 { "" } else { "s" };
        eprintln!(
            "{}",
            paint::err(
                paint::Style::DIM,
                &format!(
                    "note: {count} generated file{plural} not formatted (`{}`, the \
                     package's `generated` root)",
                    display_relative(root).display()
                )
            )
        );
    }
}

/// Says which directory links the walk stopped at, and why (G18/G19).
///
/// One dim line per link, spending the same honesty budget `exclude_generated`
/// does and for the same reason: a tool that quietly stops doing what it was
/// asked is the failure mode this design cannot afford, and here it was doing
/// too MUCH — `vilan fmt .` rewrote a stranger's tree through a link. Never a
/// warning, and never a word implying the link is illegitimate: a symlink is a
/// supported spelling of project layout (G19), and what this says is where the
/// command's own scope ends.
fn report_links_outside_the_project(links: &BTreeSet<PathBuf>) {
    for link in links {
        eprintln!(
            "{}",
            paint::err(
                paint::Style::DIM,
                &format!(
                    "note: `{}` resolves outside this project and was not walked; \
                     format that tree where it lives",
                    display_relative(link).display()
                )
            )
        );
    }
}

/// `path` relative to the working directory when it is under it, else `path` —
/// so a note about a root the user is standing in reads as they spelled it,
/// rather than as an absolute path resolution happened to produce.
fn display_relative(path: &Path) -> PathBuf {
    env::current_dir()
        .ok()
        .and_then(|cwd| {
            path.strip_prefix(vilan_core::util::canonical_path(cwd))
                .ok()
        })
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf())
}

/// An entry's identity for [`TreeWalk`]'s two guards: `(device, inode)` on
/// unix, which answers "the same entry" whatever chain of names reached it, and
/// the resolved path elsewhere — the portable spelling of the same question,
/// since Windows exposes no stable inode through `std`. Either way the key is
/// the ENTRY and never its name, which is the whole point: a cycle is one
/// directory wearing many names, and G22 is one FILE wearing two.
///
/// Directories alone until G22. The guard read the tree as though only a
/// directory could be reached twice, so a file link inside the project
/// (`src/alias.vl -> src/real.vl`, or a linked directory beside the real one)
/// handed the SAME FILE to the collector under both names: `vilan fmt --check`
/// printed two `would reformat` lines for one file and counted it twice,
/// `vilan fmt` formatted it twice, and [`manifest_fingerprint`] hashed one
/// `vilan.toml` twice. The identity is the same value for both kinds — a
/// filesystem object is a filesystem object — so the set is one set.
#[cfg(unix)]
type EntryIdentity = (u64, u64);
#[cfg(not(unix))]
type EntryIdentity = PathBuf;

/// `metadata` is the entry's own, already **followed** through any link by
/// [`TreeWalk::walk`] — which is what makes the link and its target answer
/// alike, and what keeps the guard from costing a second `stat` per entry on
/// the watcher's 300 ms poll.
#[cfg(unix)]
fn entry_identity(_path: &Path, metadata: &fs::Metadata) -> Option<EntryIdentity> {
    use std::os::unix::fs::MetadataExt;
    Some((metadata.dev(), metadata.ino()))
}

/// `fs::canonicalize` rather than [`vilan_core::util::canonical_path`], and the
/// difference is the whole guard (audit run 7's F6). `canonical_path` NEVER
/// fails: where the resolution fails it degrades to the lexical
/// `normalize_components`, which is the right answer for a comparison KEY over a
/// path that may not be on disk, and the wrong one for an IDENTITY. The two
/// agree while the resolution succeeds — and while it does, an ordinary junction
/// cycle is caught either way, because both spellings resolve to one directory,
/// and a file reached through two names resolves to one path for the same
/// reason (G22).
///
/// It is the failure that mattered, and the old code could not express it. A
/// cycle spells one directory `src/l1`, `src/l1/l1`, `src/l1/l1/l1`, …, so the
/// moment resolution stops answering, the lexical fallback mints a DISTINCT key
/// at every level: [`visited`](TreeWalk::visited) never collides, the `else` arm
/// below never runs — it was unreachable, since `Some` was the only value this
/// function could return — and the walk fans out with nothing behind it, because
/// [`TreeWalk::walk`] has no depth cap and there is no ELOOP on this side to
/// backstop it. `None` is the sentence "I cannot identify this entry", and what
/// the consumer does with it now depends on WHICH entry: a directory it cannot
/// identify is not descended into (stopping beats re-walking a tree it cannot
/// recognize), a file it cannot identify is visited anyway (a duplicate line
/// beats a source file that is never formatted). That asymmetry is
/// [`TreeWalk::walk`]'s and is spelled there; this function's job is only to say
/// honestly that it does not know. The unix arm above says the same when
/// `fs::metadata` fails, and it is the safe answer to every way resolution can
/// fail (an ACL that lets `read_dir` list a directory `CreateFileW` cannot open,
/// a volume going away mid-walk).
///
/// The verbatim (`\\?\`) prefix `canonicalize` returns is kept. This value is
/// only ever compared with another produced right here, so the one property it
/// needs is that one entry yields one key however it was reached; stripping
/// is [`vilan_core::util::canonical_path`]'s job, for the keys that have to meet
/// join-built paths.
#[cfg(not(unix))]
fn entry_identity(path: &Path, _metadata: &fs::Metadata) -> Option<EntryIdentity> {
    fs::canonicalize(path).ok()
}

/// One walk of a project tree, with the two guards a link-following walk needs
/// (G18, audit run 6's F6).
///
/// Symlinks are a SUPPORTED SPELLING of project layout — the owner's ruling, and
/// the doctrine `const.md` §9.2 now carries — so this walk **follows** a
/// directory link rather than fencing it off. Following is what needed the
/// guards, and it had neither:
///
/// * **A cycle.** `src/l1 -> .` fanned the walk out forever; with a second link
///   beside it `vilan fmt --check` never returned, and reported nothing at all,
///   since the report comes after collection. One link alone terminated only
///   because the kernel refused the fortieth resolution, after handing the same
///   file back forty-one times. The [`visited`](Self::visited) set keys on
///   directory identity, so a directory re-reached under another name is
///   recognized as one already walked.
/// * **An escape.** An ordinary directory link walked straight out of the
///   package, and `vilan fmt .` rewrote files in someone else's tree. Resolving
///   honestly is exactly what makes the scope checkable: a link is followed when
///   the tree it names is inside the resolved project, and reported (never
///   refused as illegitimate) when it is not.
///
/// The scope is the enclosing **project** — the nearest `vilan.toml` at or above
/// the walk root — rather than the walk root itself, so `vilan fmt src` still
/// follows `src/icons -> ../build/icons`, which is layout the package chose. A
/// root with no manifest above it scopes to itself: a bare directory of `.vl`
/// files is still a tree, and still not a licence to leave it.
struct TreeWalk {
    /// The resolved tree the walk may not leave.
    scope: PathBuf,
    /// Every entry already handed to the visitor, or already descended into,
    /// by [`EntryIdentity`]. One set for files and directories alike: the
    /// question "have I been here before" is the same question about both, and
    /// the answers cannot collide, since one filesystem object has one identity.
    visited: BTreeSet<EntryIdentity>,
    /// Directory links whose target resolves outside [`scope`](Self::scope), in
    /// the spelling they were reached by — what a command tells the user about
    /// rather than skipping in silence.
    outside: Vec<PathBuf>,
}

impl TreeWalk {
    fn rooted_at(root: &Path) -> TreeWalk {
        TreeWalk {
            scope: Self::scope_of(root),
            visited: BTreeSet::new(),
            outside: Vec::new(),
        }
    }

    /// The tree a walk rooted at `root` may not leave: the nearest `vilan.toml`
    /// at or above it, else the root itself.
    fn scope_of(root: &Path) -> PathBuf {
        let resolved = vilan_core::util::canonical_path(root);
        let start = if resolved.is_dir() {
            resolved
        } else {
            resolved
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| resolved.clone())
        };
        find_project_root(&start).unwrap_or(start)
    }

    /// Re-points the walk at another command-line root, KEEPING everything it
    /// has already visited (B213).
    ///
    /// G22 gave one walk one identity set — one file, one visit, whichever name
    /// reached it — and `fmt` then built a fresh walk per root, so the set did
    /// not span roots and `vilan fmt --check src src/pkg` reported every file
    /// under `src/pkg` twice. The scope is re-derived per root, because each
    /// root answers "which project is this" for itself; only the identities
    /// carry over, which is exactly the state that has to.
    fn re_root(&mut self, root: &Path) {
        self.scope = Self::scope_of(root);
    }

    /// Whether a link is part of this project — the one question the walk asks
    /// before following one. Two ways to be: the tree it resolves to is inside
    /// the resolved project, or a manifest DECLARED it as a package's
    /// `generated` root, which is a package saying that tree is its own however
    /// the filesystem spells the path there.
    ///
    /// The second arm is not a loophole in the first, it is the same rule read
    /// off the manifest instead of off the directory layout — and the watcher
    /// needs it: a generated `.vl` behind a link is a source input, and a scan
    /// that stopped at the link would compile stale bytes forever. `fmt` needs
    /// it too, for the note — those files are skipped as PRODUCTS
    /// ([`exclude_generated`]), which is what the user has to be told, rather
    /// than as a tree this command does not speak for.
    fn belongs_to_the_project(&self, link: &Path) -> bool {
        vilan_core::util::canonical_path(link).starts_with(&self.scope)
            || vilan_core::manifest::generated_root_covering(link).is_some()
    }

    /// Walks `path`, calling `visit` for every non-directory entry beneath it,
    /// in a stable (sorted) order.
    fn walk(&mut self, path: &Path, visit: &mut impl FnMut(&Path)) {
        let Ok(entry) = fs::symlink_metadata(path) else {
            return;
        };
        let metadata = if entry.is_symlink() {
            if !self.belongs_to_the_project(path) {
                self.outside.push(path.to_path_buf());
                return;
            }
            // A broken link resolves to nothing and is simply not there.
            let Ok(followed) = fs::metadata(path) else {
                return;
            };
            followed
        } else {
            entry
        };
        let identity = entry_identity(path, &metadata);
        if !metadata.is_dir() {
            // G22 — one file, one visit, whichever name reached it. A file
            // symlink inside the project resolves to the same `(device, inode)`
            // as its target, so the second spelling is recognized and dropped.
            //
            // An UNIDENTIFIABLE file is visited (the `None` arm falls through),
            // which is the opposite of the directory arm below, and deliberately
            // so: the cost of visiting a directory twice is an unbounded walk,
            // while the cost of visiting a file twice is one duplicate line — and
            // the cost of SKIPPING one is a source file the formatter never
            // formats and the watcher never watches. Each arm takes its own safe
            // direction rather than one rule taking the wrong one twice.
            if let Some(identity) = identity
                && !self.visited.insert(identity)
            {
                return;
            }
            visit(path);
            return;
        }
        let Some(identity) = identity else {
            return;
        };
        if !self.visited.insert(identity) {
            return;
        }
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        let mut children: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        children.sort();
        for child in children {
            self.walk(&child, visit);
        }
    }
}

/// Collects every `.vl` file under `path` (recursing into directories), in a
/// stable (sorted) order. Answers with the directory links the walk did not
/// follow because they leave the project ([`TreeWalk`]) — which `fmt` reports
/// and the watcher's scan ignores.
fn collect_vl_files(path: &Path, out: &mut Vec<PathBuf>) -> Vec<PathBuf> {
    let roots = [path.to_path_buf()];
    collect_vl_files_across(&roots, out)
}

/// The same collection over SEVERAL command-line roots, sharing one walk — and
/// so one identity set — across all of them (B213).
///
/// One root at a time is not the same thing: `vilan fmt --check src src/pkg`
/// walks `src`, reaches `src/pkg/helper.vl`, and then walks `src/pkg` and
/// reaches it again, because the second walk starts with an empty set. G22's
/// rule is "one file, one visit, whichever name reached it"; a per-root walk
/// only ever held it within one name.
fn collect_vl_files_across(roots: &[PathBuf], out: &mut Vec<PathBuf>) -> Vec<PathBuf> {
    let Some(first) = roots.first() else {
        return Vec::new();
    };
    let mut walk = TreeWalk::rooted_at(first);
    for root in roots {
        walk.re_root(root);
        walk.walk(root, &mut |file| {
            if file.extension().and_then(|extension| extension.to_str()) == Some("vl") {
                out.push(file.to_path_buf());
            }
        });
    }
    walk.outside
}

/// A buildable unit — a workspace member, a lone package, or a bare file: the
/// entry to compile, its package source root, the directory whose `vilan.toml`
/// declares its dependencies (for resolving the workspace), and its codegen
/// options. `name` labels a workspace member's `dist/<name>` output.
struct Unit {
    name: String,
    /// The entry file, resolved against the package root.
    entry: PathBuf,
    /// The package source root (where `import pkg::..` siblings resolve).
    pkg_root: PathBuf,
    /// The directory holding this unit's `vilan.toml` (from which its dependency
    /// workspace is resolved), or `None` for a bare file with no manifest.
    package_dir: Option<PathBuf>,
    /// `split = true` on this leg (`bundle-splitting.md` §4): emit route chunks
    /// beside the bundle. The manifest has already refused it off a browser leg.
    split: bool,
    options: BuildOptions,
    /// E119: per platform, WHY this unit is compiled under it — the clause
    /// [`vilan_core::platform_color::PlatformReason::clause`] renders. Only FILE
    /// mode fills it, because only there is the colour a conclusion the author
    /// did not write; a package leg is compiled under its own declared target,
    /// which the manifest already says out loud. Empty means "nothing to
    /// explain", and the diagnostic then names the overlay alone.
    platform_reasons: Vec<(Platform, String)>,
    /// Whether this unit's `entry` is a program the package DECLARES — a
    /// package leg, the single `[package] entry`, a bare file — rather than one
    /// of the package's MODULES addressed by path (E113's `is_package_module`),
    /// and, when it is a module, which siblings ARE declared programs (B240).
    ///
    /// Two things read it, and they used to be one: `check` skips the `main`
    /// demand and the emission walk for a module (E113), and the analysis is
    /// told which kind of entry it has been handed, so a sibling that imports
    /// the module can still import it (B239). It rides on the unit beside
    /// `platform_reasons` because it is the same kind of fact — what the
    /// manifest says about the file the caller named — and because every
    /// compile of the unit needs it, not only `check`'s.
    ///
    /// It IS `vilan_core::EntryMode`, rather than a `bool` beside one: the two
    /// were one fact spelled twice, and the declared-entry set B240 adds has
    /// exactly the shape the analysis reads.
    entry_mode: vilan_core::EntryMode,
}

/// The `[build] run` hooks of the addressed manifest (A9): external commands —
/// a Tailwind bridge, an asset pipeline, a codegen sidecar — run **before** each
/// build, in the manifest's own directory, in declaration order.
///
/// Before, because a hook exists to produce something the build then consumes
/// (generated CSS to copy, generated sources to compile); a hook that
/// post-processes the emitted bundle is a different feature and is not this one.
/// A hook that fails fails the build, naming the command: a build that silently
/// continued past a broken asset step would emit a bundle nobody asked for.
#[derive(Debug, Clone, Default)]
struct BuildHooks {
    /// The manifest's directory — the hooks' working directory, so a command's
    /// relative paths mean what they say in the file that declares them.
    dir: PathBuf,
    /// `[build] run` — the undeclared commands. They name no inputs and no
    /// outputs, so they can never be fresh and run on every build, exactly as
    /// they always have (`build-hooks.md` §3.1).
    commands: Vec<String>,
    /// `[[build.hook]]` — the named hooks, which the staleness predicate may
    /// skip. They run after every `run = [...]` command, in declaration order
    /// (§2.3).
    declared: Vec<DeclaredHook>,
}

/// One `[[build.hook]]`: a name, its commands, and what it says it reads and
/// writes. The declaration is the whole of the staleness input — §3.2 accepts,
/// in as many words, that a hook reading a file it did not declare can be
/// skipped when it should have run, and records why that trade is the right
/// one: the failure is a stale artifact rather than a wrong program (the
/// compiler hashes what it actually read), `--rerun-hooks` is the escape, and
/// the comparison is not against a sound system but against a hook that runs
/// every single time.
#[derive(Debug, Clone)]
struct DeclaredHook {
    name: String,
    commands: Vec<String>,
    inputs: Vec<String>,
    outputs: Vec<String>,
}

impl DeclaredHook {
    /// Whether this hook declared anything to be stale *about*. A hook with no
    /// `inputs` and no `outputs` is **never fresh** (§3.1) — that is today's
    /// behavior, and it is the default, so a manifest that adopts the table
    /// form without declaring paths changes nothing but the reporting.
    fn is_skippable(&self) -> bool {
        !self.inputs.is_empty() || !self.outputs.is_empty()
    }

    /// This hook's freshness fingerprint as the tree stands right now, or
    /// `None` when there is no honest answer — a declared **output** that is
    /// missing (§3.1 requires every one to exist), or a path that could not be
    /// read. `None` is never fresh and is never recorded, so the next build
    /// re-runs the hook: the one direction in which being wrong is only
    /// expensive.
    ///
    /// Content, never mtime. Not a new rule here: the watch loop's leg reuse
    /// already decides by content and says why, and a hook stamp that trusted
    /// mtime would reintroduce the bug the watcher refused.
    /// The declared inputs' digests as they are NOW. Taken BEFORE the hook
    /// runs, because the stamp must record what the hook CONSUMED: an input
    /// edited while the hook's commands are still running belongs to the next
    /// round, and a stamp that re-read the inputs afterwards would swallow
    /// that edit — the next round would find the digests equal and call the
    /// hook fresh. An unreadable input is `None` for the whole map, and such
    /// a hook is never stamped.
    fn input_digests(&self, dir: &Path) -> Option<BTreeMap<String, Option<String>>> {
        let mut inputs = BTreeMap::new();
        for declared in &self.inputs {
            // A declared input that is MISSING is recorded as missing rather
            // than skipped, so its later *appearance* invalidates — the same
            // way `asset::read`'s reader records its misses.
            inputs.insert(declared.clone(), file_digest(&dir.join(declared))?);
        }
        Some(inputs)
    }

    /// The hook's stamp: its command text, the inputs as digested by
    /// [`Self::input_digests`] (before the run), and its outputs as they are
    /// on disk NOW — after the run, when the caller is stamping a hook that
    /// ran. `None` when an output is missing: nothing is recorded for a hook
    /// whose output is missing, so it re-runs on every build.
    fn fingerprint(
        &self,
        dir: &Path,
        inputs: Option<BTreeMap<String, Option<String>>>,
    ) -> Option<HookFingerprint> {
        let inputs = inputs?;
        let mut outputs = BTreeMap::new();
        for declared in &self.outputs {
            outputs.insert(declared.clone(), file_digest(&dir.join(declared))??);
        }
        Some(HookFingerprint {
            command: digest_of(self.commands.join("\n").as_bytes()),
            inputs,
            outputs,
        })
    }

    /// The declared outputs that are not on disk, in declaration order — the
    /// ONE reason [`DeclaredHook::fingerprint`] returns `None` that is the
    /// user's mistake rather than the filesystem's, separated out so the run
    /// can say it out loud.
    ///
    /// Only a path that is genuinely absent counts (`Some(None)`): one that
    /// could not be *read* is a permission error, which is not "the hook did
    /// not write it". A link with no target IS absent — [`file_digest`]
    /// resolves through the link — and an output the build cannot follow to a
    /// file is exactly what this note is for.
    fn missing_outputs(&self, dir: &Path) -> Vec<&str> {
        self.outputs
            .iter()
            .filter(|declared| file_digest(&dir.join(declared)) == Some(None))
            .map(String::as_str)
            .collect()
    }
}

/// What a hook's stamp entry records, and what freshness compares (§3.1): the
/// digest of the command string, of every declared input (`None` for one that
/// was missing), and of every declared output. Equality of the whole structure
/// is the predicate — so adding, removing or renaming a declared path is a
/// change too, without a rule of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HookFingerprint {
    command: String,
    inputs: BTreeMap<String, Option<String>>,
    outputs: BTreeMap<String, String>,
}

/// The SHA-256 of `bytes`, lowercase hex — the same digest the release path
/// already verifies downloads with, so the toolchain hashes with one thing.
fn digest_of(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A declared path's content digest: `Some(None)` for a path that is not there,
/// `Some(Some(hex))` for one that is, and `None` when it could not be read at
/// all (a permission error) — which is not a fingerprint and must not be
/// recorded as one.
///
/// A **directory** digests as its whole tree: the sorted relative paths of
/// every member with what each one is ([`collect_tree`]), so declaring
/// `inputs = ["src/static"]` means what a reader expects it to mean. That is
/// the shape the copy case (§2.1) needs, and it is why this design refuses glob
/// patterns rather than growing a matcher.
///
/// The declared path is resolved THROUGH a symlink (`fs::metadata`), which is
/// exactly how [`insert_watched_input`] resolves the same declaration — one
/// reading of the manifest, both consumers. It stat'd the link itself until
/// G15, which was invisible for a link to a FILE (`fs::read` follows one) and a
/// silent forever-loop for a link to a DIRECTORY: not `is_dir()`, so `fs::read`
/// got `EISDIR`, so the digest was `None`, so the hook was stale on every build
/// with nothing said — while the watcher was watching the tree behind the link
/// all along. Inside a tree a link is never followed ([`collect_tree`]): that is
/// the loop-and-escape fence, and it is a different question from what the
/// declaration names.
///
/// A link with no target resolves to nothing and so reads as MISSING rather
/// than unreadable — the same answer the watcher gives it (no entry, and its
/// later appearance is a difference). Both alternatives to that are worse: an
/// unreadable input is never fresh, so the hook re-runs forever, and an
/// unreadable output is not "the hook did not write it" either, so it is never
/// reported. Missing is recorded, is explained, and invalidates when the target
/// appears.
fn file_digest(path: &Path) -> Option<Option<String>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(None),
        Err(_) => return None,
    };
    if metadata.is_dir() {
        let mut entries = Vec::new();
        collect_tree(path, Path::new(""), &mut entries)?;
        entries.sort();
        let mut joined = String::new();
        for (relative, row) in entries {
            joined.push_str(&relative);
            joined.push('\0');
            joined.push_str(&row);
            joined.push('\n');
        }
        return Some(Some(digest_of(joined.as_bytes())));
    }
    Some(Some(digest_of(&fs::read(path).ok()?)))
}

/// Every path under `root`, as `(slash-joined relative path, kind and digest)`.
/// The separator is normalized to `/` so a stamp written on one platform
/// describes the same tree on another. Three kinds of row, because the tree has
/// three kinds of member and the declaration means all of them:
///
/// * a **file** is `file <digest of its bytes>`;
/// * a **symlink** is `link <digest of its target path>`, never followed —
///   following one could walk out of the tree or into a cycle, and the link
///   itself is the declared content (the TOP-LEVEL declared path is a different
///   question and is resolved, see [`file_digest`]);
/// * a **directory** is `dir`, and its members are rows of their own.
///
/// A directory's row carries no digest because a directory has no content of
/// its own — its membership is exactly the rows of its members — so the row's
/// whole information is that the key EXISTS, which is what makes an empty one
/// visible. That row is G16: the walk pushed files and links only, so `mkdir`
/// under a declared input moved nothing here, while the watcher's
/// [`collect_input_tree`] inserts an entry per nested directory and started a
/// round for it — which this predicate then answered `Fresh`. One reading of
/// the manifest, both consumers, and it is the watcher's reading that is right:
/// N30 settled for the declared ROOT that a directory appearing is a change
/// nothing about its (absent) files can express, and a nested directory is the
/// same rule one level down. The failure the alignment removes was the safe
/// direction — a spurious round, never a missed one — but a round that
/// reliably does nothing is the gate reporting on a tree it reads differently
/// from the thing that woke it.
///
/// The KIND is in the row rather than implied by the digest, so a path that
/// changes kind is a change even where the two would otherwise hash alike: a
/// directory row of "the digest of no bytes" is indistinguishable from an empty
/// file's, and a generator replacing a directory of parts with a single file is
/// not an exotic edit.
fn collect_tree(root: &Path, prefix: &Path, out: &mut Vec<(String, String)>) -> Option<()> {
    let mut children: Vec<PathBuf> = fs::read_dir(root)
        .ok()?
        .map(|entry| entry.ok().map(|entry| entry.path()))
        .collect::<Option<Vec<_>>>()?;
    children.sort();
    for child in children {
        let name = child.file_name()?.to_string_lossy().into_owned();
        let relative = prefix.join(&name);
        let metadata = fs::symlink_metadata(&child).ok()?;
        let key = relative.to_string_lossy().replace('\\', "/");
        if metadata.is_symlink() {
            let target = fs::read_link(&child).ok()?;
            let digest = digest_of(target.to_string_lossy().as_bytes());
            out.push((key, format!("link {digest}")));
        } else if metadata.is_dir() {
            out.push((key, "dir".to_string()));
            collect_tree(&child, &relative, out)?;
        } else {
            let digest = digest_of(&fs::read(&child).ok()?);
            out.push((key, format!("file {digest}")));
        }
    }
    Some(())
}

impl BuildHooks {
    fn from_manifest(dir: &Path, manifest: &Manifest) -> BuildHooks {
        BuildHooks {
            dir: dir.to_path_buf(),
            commands: manifest.build_hooks().to_vec(),
            declared: manifest
                .declared_hooks()
                .iter()
                .map(|hook| DeclaredHook {
                    // `validate` has already refused a hook with no name, so
                    // the fallback is unreachable rather than a policy.
                    name: hook.name.clone().unwrap_or_default(),
                    commands: hook.commands().to_vec(),
                    inputs: hook.inputs().to_vec(),
                    outputs: hook.outputs().to_vec(),
                })
                .collect(),
        }
    }

    /// Where this project's hook stamps live: `dist/.build-hooks.json` in the
    /// project's own directory (`build-hooks.md` §3.3, ruled by the owner as
    /// Q2 on 2026-08-28). Not `~/.vilan/`: a machine-global cache keyed on a
    /// project path is the thing nobody can reason about from a fresh clone,
    /// and a stale one is unreachable to `rm -rf`. Here, `rm -rf dist` means
    /// *rebuild everything, hooks included* — the sentence a user already
    /// believes.
    fn stamp_path(&self) -> PathBuf {
        self.dir.join("dist").join(".build-hooks.json")
    }

    /// Runs every hook, in order, stopping at the first failure. Each command
    /// goes through the **platform shell** (`sh -c` / `cmd /C`) — hooks are
    /// shell one-liners with globs, pipes and `&&`, and an argv array would make
    /// the user hand-split them and lose all three. Streams are inherited, so a
    /// hook's output (and its TTY colors) reach the terminal as if run by hand;
    /// under `vilan build --stdout` that means a chatty hook shares the JS
    /// stream — redirect it in the command if that matters.
    ///
    /// The trust model is deliberate and **first-party** (E96, ruled
    /// 2026-08-26; `proposal/build-trust.md`): a hook is code the developer
    /// wrote in their own manifest, so it runs with their privileges and
    /// environment — no sandbox, no allowlist, no timeout, no consent prompt —
    /// the same trust `cargo build` and `npm run` already take. The echo below
    /// is the whole honesty budget: the terminal always names what ran. A
    /// first-run consent gate was proposed and **declined**; don't add one.
    /// Only the addressed manifest contributes hooks ([`Project::hooks`]) — a
    /// dependency's are never reached, which is what keeps this tier
    /// first-party. That is the other tier, and it is now spelled rather than
    /// merely reserved: a dependency grants it with `build-hooks = true` on
    /// its own declaration, absent means no, **nothing honors the grant yet**,
    /// and [`note_refused_dependency_hooks`] says so out loud
    /// (`build-hooks.md` §4.3, §8's S2; the owner's Q6 ruling of 2026-08-28).
    ///
    /// The `[build] run` commands above run unconditionally; the
    /// `[[build.hook]]` tables below may be **skipped**, and the freshness
    /// predicate is [`DeclaredHook::fingerprint`] compared against the stamp.
    /// Freshness is a cost optimization over code that is already trusted to
    /// run, and must never be described as a security property: if a hook is
    /// dangerous, running it once is the whole of the damage.
    fn run(&self, rerun: bool) -> Result<(), String> {
        // Before anything runs, and unconditionally — a hook that is skipped as
        // fresh, or that fails, still declared what it reads, and under
        // `--watch` the edit to one of those files is precisely the event that
        // must start the next round.
        self.record_watched_inputs();
        for command in &self.commands {
            self.spawn(command, "`[build] run`")?;
        }
        // A bare file has no manifest directory, so it has neither hooks nor a
        // `dist/` to stamp in — and resolving `dist/.build-hooks.json` against
        // an empty path would name one in the working directory.
        if self.dir.as_os_str().is_empty() {
            return Ok(());
        }
        // Deliberately NOT short-circuited on an empty declaration list: the
        // stamp is a function of what the manifest says today, so a manifest
        // that drops its last `[[build.hook]]` has to take the stamp with it.
        // The write path removes rather than creates when there is nothing to
        // record, so a `run = [...]`-only project still grows no `dist/`.
        let stamp_path = self.stamp_path();
        let mut recorded = read_hook_stamp(&stamp_path);
        // Built from the CURRENT declarations only, so a hook removed from the
        // manifest takes its entry with it and the file stays a function of
        // what the manifest says today.
        let mut next: BTreeMap<String, HookFingerprint> = BTreeMap::new();
        for hook in &self.declared {
            let label = format!("`[[build.hook]]` `{}`", hook.name);
            let inputs_before = hook.input_digests(&self.dir);
            let before = hook.fingerprint(&self.dir, inputs_before.clone());
            let fresh = !rerun
                && hook.is_skippable()
                && before.is_some()
                && recorded.remove(&hook.name) == before;
            // Recorded whichever way it goes, and in the words the terminal
            // uses: `--explain` names a hook's declared outputs and this
            // build's verdict for each, which is the difference between "this
            // file is current" and "this file is what the last run left".
            explain::hook(
                &hook.name,
                if fresh { "Fresh" } else { "ran" },
                hook.inputs.iter().map(|path| self.dir.join(path)).collect(),
                hook.outputs
                    .iter()
                    .map(|path| self.dir.join(path))
                    .collect(),
            );
            if fresh {
                eprintln!(
                    "{}",
                    paint::err(paint::Style::DIM, &format!("Fresh   {}", hook.name))
                );
                // A skipped hook keeps its stamp: skipping is not a reason to
                // forget why it was skipped.
                next.insert(
                    hook.name.clone(),
                    before.expect("fresh implies a fingerprint"),
                );
                continue;
            }
            for command in &hook.commands {
                if let Err(error) = self.spawn(command, &label) {
                    // A failing hook leaves NO stamp of its own, so the next
                    // build re-runs it — but the hooks that already succeeded
                    // this round keep theirs, because they did happen.
                    write_hook_stamp(&stamp_path, &next);
                    return Err(error);
                }
            }
            // A hook that succeeded and left a declared output missing is the
            // one way this design can go quiet: nothing is stamped, so it
            // re-runs forever, and the build fails later at whatever was
            // supposed to consume the file — an import error with no path back
            // to its cause. The manifest said what this hook produces; when it
            // does not, say so here, where the reason is still known. A note,
            // not an error: the hook itself succeeded, and the build's own
            // outcome is the compile's to decide.
            for missing in hook.missing_outputs(&self.dir) {
                eprintln!(
                    "{} `[[build.hook]]` `{}` did not write its declared `outputs` entry \
                     `{missing}`: nothing is recorded for a hook whose output is missing, so \
                     it re-runs on every build — write the file, or drop it from `outputs`",
                    paint::warning_prefix(),
                    hook.name
                );
            }
            // Fingerprinted AFTER the run: the outputs recorded are the ones
            // the run actually produced. A hook that did not produce a
            // declared output records nothing and re-runs next build.
            // Inputs as digested BEFORE the run, outputs as written by it: an
            // input edited while the commands ran is the next round's, not this
            // stamp's (the race Windows CI exposed at Order 25's seal).
            if let Some(after) = hook.fingerprint(&self.dir, inputs_before) {
                next.insert(hook.name.clone(), after);
            }
        }
        write_hook_stamp(&stamp_path, &next);
        Ok(())
    }

    /// Hands every declared `[[build.hook]]` input to the watcher's
    /// recorded-input set (G10).
    ///
    /// The freshness stamp and the `--watch` wake-up set are two consumers of
    /// ONE declaration, and before this they disagreed: a manifest could name a
    /// file in `inputs`, have the stamp re-run the hook the moment its bytes
    /// moved, and never get a round in which that could happen — the loop
    /// polled `.vl` sources only, so editing a declared input produced nothing
    /// at all until some unrelated `.vl` save woke the session. The paths are
    /// resolved against [`BuildHooks::dir`], the same base
    /// [`DeclaredHook::fingerprint`] resolves them against, so an input outside
    /// the watch root (`inputs = ["../shared/icons"]`) is watched exactly as it
    /// is stamped.
    ///
    /// `outputs` are not recorded, on purpose — see [`RECORDED_INPUT_PATHS`].
    ///
    /// Recorded as [`InputReading::Tree`], which is the declaration's own
    /// reading: [`file_digest`] hashes a declared directory as its whole tree,
    /// so `inputs = ["src/static"]` means what it looks like it means, and the
    /// round has to wake for every edit that re-stales the hook.
    fn record_watched_inputs(&self) {
        record_watched_inputs(
            self.declared
                .iter()
                .flat_map(|hook| hook.inputs.iter())
                .map(|declared| (self.dir.join(declared), InputReading::Tree)),
        );
    }

    /// Runs one command through the platform shell, echoing it first. `label`
    /// is the manifest key at fault, so a failure names the form the user
    /// actually wrote rather than always blaming `[build] run`.
    fn spawn(&self, command: &str, label: &str) -> Result<(), String> {
        eprintln!("{} {command}", paint::err(paint::Style::CYAN, "Running"));
        let spawned = shell_command(command).current_dir(&self.dir).spawn();
        let mut child = match spawned {
            // The Job object costs nothing here and buys the Windows
            // tree-kill: a hook that spawns a watcher of its own dies with
            // this process instead of outliving the session.
            Ok(child) => ManagedChild::adopt(child),
            Err(error) => {
                return Err(format!("{label} could not start `{command}`: {error}"));
            }
        };
        match child.wait() {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(format!(
                "{label} command failed ({}): {command}",
                status
                    .code()
                    .map(|code| format!("exit code {code}"))
                    .unwrap_or_else(|| "killed by a signal".to_string())
            )),
            Err(error) => Err(format!("{label} lost `{command}`: {error}")),
        }
    }
}

/// The stamp's format version. It is a *string* so the reader below never has
/// to understand JSON numbers; bump it when the recorded shape changes, and an
/// older or newer stamp then reads as no stamp — every hook re-runs once,
/// which is the only direction in which being wrong is merely expensive.
const HOOK_STAMP_VERSION: &str = "1";

/// Reads `dist/.build-hooks.json`. **Every failure reads as "no stamp"** — a
/// missing file, a truncated write, a hand edit, a version this binary does not
/// know. That is deliberate and it is what makes the whole feature safe to
/// ship: the worst a corrupt stamp can do is make the build run the hooks it
/// would have run before any of this existed.
fn read_hook_stamp(path: &Path) -> BTreeMap<String, HookFingerprint> {
    let mut stamps = BTreeMap::new();
    let Ok(text) = fs::read_to_string(path) else {
        return stamps;
    };
    let Some(Json::Object(root)) = parse_json(&text) else {
        return stamps;
    };
    if root.get("version") != Some(&Json::Text(HOOK_STAMP_VERSION.to_string())) {
        return stamps;
    }
    let Some(Json::Object(hooks)) = root.get("hooks") else {
        return stamps;
    };
    for (name, entry) in hooks {
        let Json::Object(entry) = entry else {
            return BTreeMap::new();
        };
        let (Some(Json::Text(command)), Some(Json::Object(inputs)), Some(Json::Object(outputs))) = (
            entry.get("command"),
            entry.get("inputs"),
            entry.get("outputs"),
        ) else {
            return BTreeMap::new();
        };
        let mut recorded_inputs = BTreeMap::new();
        for (declared, digest) in inputs {
            match digest {
                Json::Text(digest) => {
                    recorded_inputs.insert(declared.clone(), Some(digest.clone()))
                }
                Json::Null => recorded_inputs.insert(declared.clone(), None),
                _ => return BTreeMap::new(),
            };
        }
        let mut recorded_outputs = BTreeMap::new();
        for (declared, digest) in outputs {
            let Json::Text(digest) = digest else {
                return BTreeMap::new();
            };
            recorded_outputs.insert(declared.clone(), digest.clone());
        }
        stamps.insert(
            name.clone(),
            HookFingerprint {
                command: command.clone(),
                inputs: recorded_inputs,
                outputs: recorded_outputs,
            },
        );
    }
    stamps
}

/// Writes the stamp back, or removes it when nothing is left to record — the
/// same rule the asset-kind record follows, so the stamp never becomes its own
/// stale artifact. A failure is reported and otherwise ignored: the stamp is
/// bookkeeping for the NEXT build's cost, never this build's correctness.
///
/// `dist/` is created if it is not there. That happens only when the manifest
/// declares a `[[build.hook]]`, so a project that writes only `run = [...]`
/// grows no directory it did not have before.
fn write_hook_stamp(path: &Path, stamps: &BTreeMap<String, HookFingerprint>) {
    if stamps.is_empty() {
        if path.is_file()
            && let Err(error) = fs::remove_file(path)
        {
            eprintln!(
                "{} cannot remove the hook stamp {}: {error}",
                paint::warning_prefix(),
                path.display()
            );
        }
        return;
    }
    let entries = stamps
        .iter()
        .map(|(name, fingerprint)| {
            let inputs = fingerprint
                .inputs
                .iter()
                .map(|(declared, digest)| {
                    format!(
                        "\t\t\t\t{}: {}",
                        json_string(declared),
                        digest
                            .as_deref()
                            .map_or_else(|| "null".to_string(), json_string)
                    )
                })
                .collect::<Vec<_>>()
                .join(",\n");
            let outputs = fingerprint
                .outputs
                .iter()
                .map(|(declared, digest)| {
                    format!("\t\t\t\t{}: {}", json_string(declared), json_string(digest))
                })
                .collect::<Vec<_>>()
                .join(",\n");
            format!(
                "\t\t{}: {{\n\t\t\t\"command\": {},\n\t\t\t\"inputs\": {},\n\t\t\t\"outputs\": {}\n\t\t}}",
                json_string(name),
                json_string(&fingerprint.command),
                wrap_json_object(&inputs),
                wrap_json_object(&outputs),
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let text = format!(
        "{{\n\t\"version\": {},\n\t\"hooks\": {{\n{entries}\n\t}}\n}}\n",
        json_string(HOOK_STAMP_VERSION)
    );
    if let Some(directory) = path.parent()
        && let Err(error) = fs::create_dir_all(directory)
    {
        eprintln!(
            "{} cannot create {}: {error}",
            paint::warning_prefix(),
            directory.display()
        );
        return;
    }
    if let Err(error) = fs::write(path, text) {
        eprintln!(
            "{} cannot write the hook stamp {}: {error}",
            paint::warning_prefix(),
            path.display()
        );
    }
}

/// `{}` for an empty body, or the rows wrapped and closed at the entry's
/// indentation.
fn wrap_json_object(rows: &str) -> String {
    if rows.is_empty() {
        "{}".to_string()
    } else {
        format!("{{\n{rows}\n\t\t\t}}")
    }
}

/// The JSON subset the hook stamp is written in: objects, strings and `null`
/// (arrays are parsed too, so the reader does not choke on a future field it
/// merely has to ignore). Numbers and booleans are deliberately absent —
/// nothing writes them, and refusing them costs a re-run rather than a wrong
/// answer, which is why the stamp's own version is a string.
///
/// Hand-rolled rather than pulling a JSON crate into the `vilan` binary: the
/// tree already hand-writes JSON here (`json_string`, whose comment records the
/// same "correct, not clever" standard), the grammar this has to read is one
/// this file also writes, and a parse failure has a safe answer.
#[derive(Debug, PartialEq, Eq)]
enum Json {
    Null,
    Text(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

/// Parses `text` as one JSON value, requiring the whole input to be consumed.
/// `None` for anything that does not parse.
fn parse_json(text: &str) -> Option<Json> {
    let mut characters = text.chars().peekable();
    let value = parse_json_value(&mut characters, 0)?;
    skip_json_whitespace(&mut characters);
    characters.peek().is_none().then_some(value)
}

/// Guards the one unbounded thing in the grammar. A stamp is two levels deep;
/// a file crafted to be a thousand is not a stamp, and recursing on it would
/// be the parser's problem rather than the format's.
const JSON_MAX_DEPTH: usize = 16;

fn skip_json_whitespace(characters: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while characters.next_if(|c| c.is_ascii_whitespace()).is_some() {}
}

fn parse_json_value(
    characters: &mut std::iter::Peekable<std::str::Chars<'_>>,
    depth: usize,
) -> Option<Json> {
    if depth > JSON_MAX_DEPTH {
        return None;
    }
    skip_json_whitespace(characters);
    match characters.peek()? {
        '"' => parse_json_string(characters).map(Json::Text),
        'n' => {
            for expected in "null".chars() {
                if characters.next()? != expected {
                    return None;
                }
            }
            Some(Json::Null)
        }
        '[' => {
            characters.next();
            let mut items = Vec::new();
            skip_json_whitespace(characters);
            if characters.next_if_eq(&']').is_some() {
                return Some(Json::Array(items));
            }
            loop {
                items.push(parse_json_value(characters, depth + 1)?);
                skip_json_whitespace(characters);
                match characters.next()? {
                    ',' => {}
                    ']' => return Some(Json::Array(items)),
                    _ => return None,
                }
            }
        }
        '{' => {
            characters.next();
            let mut fields = BTreeMap::new();
            skip_json_whitespace(characters);
            if characters.next_if_eq(&'}').is_some() {
                return Some(Json::Object(fields));
            }
            loop {
                skip_json_whitespace(characters);
                let key = parse_json_string(characters)?;
                skip_json_whitespace(characters);
                if characters.next()? != ':' {
                    return None;
                }
                fields.insert(key, parse_json_value(characters, depth + 1)?);
                skip_json_whitespace(characters);
                match characters.next()? {
                    ',' => {}
                    '}' => return Some(Json::Object(fields)),
                    _ => return None,
                }
            }
        }
        _ => None,
    }
}

/// One JSON string literal, undoing exactly the escapes [`json_string`] emits
/// plus the rest of the standard set, so a stamp written by this binary always
/// round-trips.
fn parse_json_string(characters: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    if characters.next()? != '"' {
        return None;
    }
    let mut text = String::new();
    loop {
        match characters.next()? {
            '"' => return Some(text),
            '\\' => match characters.next()? {
                '"' => text.push('"'),
                '\\' => text.push('\\'),
                '/' => text.push('/'),
                'b' => text.push('\u{8}'),
                'f' => text.push('\u{c}'),
                'n' => text.push('\n'),
                'r' => text.push('\r'),
                't' => text.push('\t'),
                'u' => {
                    let mut code = 0u32;
                    for _ in 0..4 {
                        code = code * 16 + characters.next()?.to_digit(16)?;
                    }
                    // A lone surrogate is not a character; the stamp never
                    // writes one, so refusing is the right answer.
                    text.push(char::from_u32(code)?);
                }
                _ => return None,
            },
            character => text.push(character),
        }
    }
}

/// One hook command, wrapped for the platform shell.
fn shell_command(command: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        let mut shell = std::process::Command::new("cmd");
        shell.arg("/C").arg(command);
        shell
    }
    #[cfg(not(windows))]
    {
        let mut shell = std::process::Command::new("sh");
        shell.arg("-c").arg(command);
        shell
    }
}

/// How a workspace designates the Node leg `vilan run` executes without
/// `--entry` (A15's follow-up: kolt needs no flag). The two workspace shapes
/// spell the designation in their own section — `[package] default-entry` names
/// an `[entry.<name>]`, `[project] default-entry` names a member package — so
/// the key travels with the name and every message quotes the one the user
/// would actually write.
#[derive(Debug, Clone)]
struct DefaultEntry {
    key: &'static str,
    name: Option<String>,
}

impl DefaultEntry {
    fn new(key: &'static str, name: Option<&str>) -> DefaultEntry {
        DefaultEntry {
            key,
            name: name.map(str::to_string),
        }
    }
}

/// A project to act on: a lone package / bare file (its platform chosen with the
/// `--platform` flag, defaulting to the package's), or a workspace of members each
/// built for its own fixed platform. The legacy `[server]`/`[client]` pair lowers
/// onto a two-member workspace.
enum Project {
    Single {
        unit: Unit,
        /// The package's declared `target` platform, if any (`None` ⇒ the `node`
        /// default). In file mode this is the FIRST color the build compiles the
        /// addressed file under ([`vilan_core::platform_color::file_platforms`]).
        platform: Option<Platform>,
        /// The further colors the build compiles this file under: a module
        /// shared between legs of a multi-entry package is compiled once per
        /// leg and must type-check under each, so `check` covers every one of
        /// them and reports the union (E113). Empty for everything else —
        /// including a build, which writes one artifact and so uses `platform`
        /// alone.
        shared_platforms: Vec<Platform>,
        /// The `[build] run` hooks to run before building it (A9).
        hooks: BuildHooks,
    },
    Workspace {
        root: PathBuf,
        members: Vec<(Unit, Platform)>,
        /// Which member `vilan run` executes without `--entry` (A15's
        /// follow-up).
        default_entry: DefaultEntry,
        /// The `[build] run` hooks to run before building it (A9).
        hooks: BuildHooks,
    },
    /// A standalone `[library]`, addressed directly. Not a buildable app (a library
    /// is compiled only as a dependency), but `vilan check` verifies its platform
    /// contract. `dir` is the library's package directory; `name` labels diagnostics.
    Library { dir: PathBuf, name: String },
}

impl Project {
    /// The `[build] run` hooks to run before building this project (A9). A
    /// `[library]` is never built on its own, so it declares none here.
    fn hooks(&self) -> Option<&BuildHooks> {
        match self {
            Project::Single { hooks, .. } | Project::Workspace { hooks, .. } => Some(hooks),
            Project::Library { .. } => None,
        }
    }

    /// The build units this project compiles — the one place that knows how to
    /// see past the two workspace shapes, so a question asked of "every package
    /// in this build" is asked once. A `[library]` addressed on its own is not
    /// built, so it has none.
    fn units(&self) -> impl Iterator<Item = &Unit> {
        let (single, members) = match self {
            Project::Single { unit, .. } => (Some(unit), [].as_slice()),
            Project::Workspace { members, .. } => (None, members.as_slice()),
            Project::Library { .. } => (None, [].as_slice()),
        };
        single
            .into_iter()
            .chain(members.iter().map(|(unit, _)| unit))
    }
}

/// Resolves the project from an optional path, then runs `action`. An explicit
/// file is a single entry; a directory (or no path, via the working directory)
/// is read from its `vilan.toml`.
fn with_project<T: CommandFailure>(path: Option<PathBuf>, action: impl FnOnce(Project) -> T) -> T {
    match resolve_project(path) {
        Ok(project) => action(project),
        Err(message) => report_error(&message),
    }
}

fn resolve_project(path: Option<PathBuf>) -> Result<Project, String> {
    match path {
        // An explicit directory: the project rooted there.
        Some(path) if path.is_dir() => project_from_manifest(&path),
        // An explicit file (or a not-yet-existing path, so `compile` can report
        // the read error): a single entry, compiled as a file OF the package
        // that owns it.
        Some(path) => file_project(path),
        // No path: find the enclosing project from the working directory.
        None => {
            let working_dir = env::current_dir()
                .map_err(|error| format!("cannot read the working directory: {error}"))?;
            let root = find_project_root(&working_dir).ok_or_else(|| {
                "no `vilan.toml` found here or in any parent directory; \
                 pass a source file to compile it directly"
                    .to_string()
            })?;
            project_from_manifest(&root)
        }
    }
}

/// The package that owns a file addressed by path: the nearest `vilan.toml` at
/// or above its directory, read and **validated**, with the directory holding
/// it. `Ok(None)` is a genuinely manifest-less file — a scratch program outside
/// any project.
///
/// One discovery for every path-addressed command, so `vilan check src/main.vl`,
/// `vilan build src/main.vl` and `vilan test src/main_test.vl` cannot come to
/// three different answers about which package a file belongs to. `vilan test`
/// has resolved a test file this way since distribution.md §7's S4 — a test file
/// is a file *of* its package — and G20 is what that leaves: every other file
/// mode still built a package-less unit.
fn owning_package(file: &Path) -> Result<Option<(PathBuf, Manifest)>, String> {
    let Some(directory) = file.parent().and_then(find_project_root) else {
        return Ok(None);
    };
    let (manifest, _warnings) = read_manifest_quietly(&directory)?;
    Ok(Some((directory, manifest)))
}

/// The project a file addressed by path compiles under (G20, audit run 6's F11).
///
/// Before this, an explicit file built a `Unit` with **no** `package_dir`, so the
/// manifest was never read in file mode — and every answer that depends on it
/// was a lie the user could not see. Three, measured: a package already on
/// `prelude = "std::web"` was steered to "set `prelude = \"std::web\"`", the edit
/// it had already made; `prelude = false` was silently ineffective, so a name the
/// package removed still resolved; and a manifest the validator refuses passed
/// file-mode check wordlessly while directory mode failed the build on it.
///
/// What the file adopts is what the package IS — its source root, dependency
/// workspace, prelude, build options, and (the language server's rule, so the
/// editor and the terminal agree) its platform when the file lives under the
/// package's source root. What it does not adopt is what addressing the
/// DIRECTORY means: `[build]` hooks do not run, because naming one file is a
/// request to compile that file and not to drive the package's build pipeline,
/// and a shell command is not a side effect to acquire by accident.
///
/// A file with no manifest above it keeps its old context exactly: its own
/// directory as the package root, no dependencies, default options — and so does
/// a file under a `[project]` or `[library]` root, which has no `[package]` to
/// belong to.
fn file_project(entry: PathBuf) -> Result<Project, String> {
    let bare = |entry: PathBuf| Project::Single {
        unit: Unit {
            name: String::new(),
            pkg_root: pkg_root_of(&entry),
            entry,
            package_dir: None,
            split: false,
            options: BuildOptions::default(),
            // No project to colour it: the CLI's `node` default answers, and
            // there is nothing about the file's own situation to explain.
            platform_reasons: Vec::new(),
            // A file with no `[package]` above it IS the program it names.
            entry_mode: vilan_core::EntryMode::Declared,
        },
        platform: None,
        shared_platforms: Vec::new(),
        hooks: BuildHooks::default(),
    };
    let Some((directory, manifest)) = owning_package(&entry)? else {
        return Ok(bare(entry));
    };
    let Some(package) = manifest.package.as_ref() else {
        return Ok(bare(entry));
    };
    let options = manifest
        .build_options()
        .map_err(|error| format!("invalid {}/vilan.toml: {error}", directory.display()))?;
    let pkg_root = directory.join(package.root());
    // The platform, by the one rule the language server also takes
    // (`platform_color::file_platforms`): the classic single-entry form colors
    // every file under its source root, and a multi-entry package colors a file
    // by the entry that REACHES it — the build's own question, since a build is
    // one compile per entry over the modules that entry loads. Several reaching
    // legs give several colors, and `check_once` covers each; none, and the
    // designated `default-entry` answers. A file outside the source root is not
    // the package's to color — it still resolves `pkg::` and the dependencies,
    // which is what it needs.
    // Each colour with the REASON it was chosen (E119): a file addressed by path
    // is coloured by something the author did not write — which entry reaches
    // it, or which one the manifest designates — and a type-level diagnostic
    // that follows from the colour is unreadable without it.
    let choices = vilan_core::platform_color::file_platform_choices(&pkg_root, &manifest, &entry);
    let platform_reasons: Vec<(Platform, String)> = choices
        .iter()
        .map(|choice| (choice.platform, choice.reason.clause()))
        .collect();
    let mut platforms = choices.into_iter().map(|choice| choice.platform);
    let platform = platforms.next();
    let shared_platforms: Vec<Platform> = platforms.collect();
    // B239/B240: which situation this compile is in, and — in file mode — which
    // of the package's files are programs a module may not import.
    let entry_mode = match is_package_module(&pkg_root, &manifest, &entry) {
        false => vilan_core::EntryMode::Declared,
        true => vilan_core::EntryMode::OpenFile {
            declared_entries: vilan_core::platform_color::declared_entry_module_names(&manifest),
        },
    };
    Ok(Project::Single {
        unit: Unit {
            name: String::new(),
            entry,
            pkg_root,
            package_dir: Some(directory),
            split: false,
            options,
            platform_reasons,
            entry_mode,
        },
        platform,
        shared_platforms,
        hooks: BuildHooks::default(),
    })
}

/// Whether `file` is a MODULE of the package: under its source root, and not
/// one of its declared program entries (the single `[package] entry`, default
/// `main.vl`, or an `[entry.<name>]` path). A module has no `main`, and nothing
/// should ask it for one (E113).
///
/// The rule itself lives in `vilan_core::platform_color` beside
/// `file_platform_choices`, because the language server asks the same question
/// about the same file and the two surfaces must not answer it twice (B239 —
/// the editor reads it to say whether the analyzed entry is a declared program
/// or one of the package's modules opened as one).
fn is_package_module(pkg_root: &Path, manifest: &Manifest, file: &Path) -> bool {
    vilan_core::platform_color::is_package_module(pkg_root, manifest, file)
}

/// Reads, parses, validates, and reports warnings for the `vilan.toml` in
/// `directory`.
fn read_manifest(directory: &Path) -> Result<Manifest, String> {
    let (manifest, warnings) = read_manifest_quietly(directory)?;
    for warning in &warnings {
        eprintln!(
            "{} {} in {}",
            paint::warning_prefix(),
            warning,
            directory.join("vilan.toml").display()
        );
    }
    Ok(manifest)
}

/// The use-site warning head for a renamed CLI spelling (deprecation.md §4;
/// diagnostics ledger row 247): the family form the compiler's `[deprecated]`
/// warning carries, with spellings substituted. Spanless — the caller prints
/// it as a plain stderr line behind `paint::warning_prefix()`, no ariadne
/// panel (there is no span).
#[cfg_attr(not(test), allow(dead_code))]
fn deprecated_spelling_warning(old_spelling: &str, new_spelling: &str) -> String {
    format!("`{old_spelling}` is deprecated; use `{new_spelling}`")
}

/// A renamed flag under the one-minor window (deprecation.md §4). clap's
/// `alias` cannot warn — it does not record which spelling matched — so a
/// rename keeps the OLD spelling as its own hidden arg
/// (`#[arg(long, hide = true)]`), reconciled here at dispatch: the old
/// spelling present warns and folds its value into the new arg; both present
/// with CONFLICTING values is an error. Returns the effective value plus the
/// warning line to print, `Err` with the refusal otherwise.
///
/// No real rename exists today (`--target` is a documented courtesy alias
/// with no removal intended — not a deprecation, deliberately silent), so
/// nothing outside the tests calls this yet: the mechanism is exercised on a
/// SYNTHETIC pair there, and the first real rename wires its own hidden arg
/// through this function. A renamed SUBCOMMAND takes the same shape — a
/// hidden variant that prints [`deprecated_spelling_warning`] and delegates.
#[cfg_attr(not(test), allow(dead_code))]
fn reconcile_renamed_flag<T: PartialEq>(
    new: Option<T>,
    old: Option<T>,
    new_spelling: &str,
    old_spelling: &str,
) -> Result<(Option<T>, Option<String>), String> {
    match (new, old) {
        (new, None) => Ok((new, None)),
        (None, Some(old_value)) => Ok((
            Some(old_value),
            Some(deprecated_spelling_warning(old_spelling, new_spelling)),
        )),
        (Some(new_value), Some(old_value)) => {
            if new_value == old_value {
                Ok((
                    Some(new_value),
                    Some(deprecated_spelling_warning(old_spelling, new_spelling)),
                ))
            } else {
                Err(format!(
                    "`{old_spelling}` is a deprecated spelling of `{new_spelling}`, and both \
                     are given with different values; drop `{old_spelling}`"
                ))
            }
        }
    }
}

/// The same read without the warning report, for a caller that runs **per
/// file** — `vilan test` reads its package's manifest once per test, and one
/// unknown key should not print once per test.
fn read_manifest_quietly(directory: &Path) -> Result<(Manifest, Vec<String>), String> {
    let manifest_path = directory.join("vilan.toml");
    let contents = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let (manifest, warnings) = Manifest::parse(&contents)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    let errors = manifest.validate();
    if !errors.is_empty() {
        return Err(format!(
            "invalid {}:\n  - {}",
            manifest_path.display(),
            errors.join("\n  - ")
        ));
    }
    Ok((manifest, warnings))
}

/// Builds a [`Unit`] from a package manifest in `directory`.
fn unit_from_package(directory: &Path, package: &Package, options: BuildOptions) -> Unit {
    let pkg_root = directory.join(package.root());
    Unit {
        name: package.name.clone().unwrap_or_default(),
        entry: pkg_root.join(package.entry()),
        pkg_root,
        package_dir: Some(directory.to_path_buf()),
        split: package.splits(),
        options,
        // A package leg is compiled under its own declared `target`, which the
        // manifest says out loud — nothing for E119 to explain.
        platform_reasons: Vec::new(),
        // The `[package] entry` itself: the program the manifest declares.
        entry_mode: vilan_core::EntryMode::Declared,
    }
}

/// The build units a `[package]` manifest contributes: one per `[entry.<name>]`
/// when declared (proposal/platform-coloring.md §4.2), else the single classic
/// unit. Entry units build browser-class entries FIRST (stable within a class)
/// — the order is semantic, so a process entry that serves bundles always
/// finds them freshly built, whatever order the manifest declares.
fn package_units(
    directory: &Path,
    package: &Package,
    manifest: &Manifest,
    options: BuildOptions,
) -> Vec<(Unit, Platform)> {
    if manifest.entries.is_empty() {
        let platform = package.resolved_target().unwrap_or_default();
        return vec![(unit_from_package(directory, package, options), platform)];
    }
    let pkg_root = directory.join(package.root());
    let mut units: Vec<(Unit, Platform)> = manifest
        .entries
        .iter()
        .map(|(name, entry)| {
            (
                Unit {
                    name: name.clone(),
                    entry: pkg_root.join(entry.path(name)),
                    pkg_root: pkg_root.clone(),
                    package_dir: Some(directory.to_path_buf()),
                    split: entry.splits(),
                    options,
                    // As above: this leg's `[entry.<name>] target` IS the
                    // explanation, and the author wrote it.
                    platform_reasons: Vec::new(),
                    // An `[entry.<name>]` path: declared, by name.
                    entry_mode: vilan_core::EntryMode::Declared,
                },
                entry.resolved_target().unwrap_or_default(),
            )
        })
        .collect();
    units.sort_by_key(|(_, platform)| !matches!(platform, Platform::Browser));
    units
}

/// The `dist/` bundle one build unit writes. The extension is the platform's
/// (`Platform::script_extension`) — a process runtime is handed a `.mjs` so it
/// classifies the ESM we emit without sniffing it; the browser keeps `.js`,
/// since its `<script type="module">` tag already declares the module. One
/// definition, used by the writers and by every path that later launches or
/// reports the artifact, so a name can never be reconstructed two ways.
fn artifact_path(dist: &Path, name: &str, platform: Platform) -> PathBuf {
    dist.join(format!("{name}.{}", platform.script_extension()))
}

/// E92: the bundle just written at `output` may sit beside a SUPERSEDED
/// generation — the same stem under the other script classification's
/// extension (`dist/server.js` beside the `dist/server.mjs` every
/// post-v0.33.0 build writes, or a stranded `.mjs` after a leg retargeted to
/// the browser). Within one build the `<name>.*` namespace belongs to one
/// leg (`reject_output_collisions`), so a surviving other-classification
/// sibling can only be an earlier generation's artifact — and a script,
/// Dockerfile, or process manager still naming it keeps launching the
/// superseded application silently, exactly the drift the gotchas page
/// warns about. Nothing here deletes: no record proves the build wrote the
/// old file (pre-rename builds recorded nothing), and the output directory
/// is not exclusively the build's — the drift is reported instead, with the
/// fix in the message.
fn warn_superseded_sibling(output: &Path) {
    let Some(extension) = output.extension().and_then(|extension| extension.to_str()) else {
        return;
    };
    let retired_extension = match extension {
        "mjs" => "js",
        "js" => "mjs",
        _ => return,
    };
    let superseded = output.with_extension(retired_extension);
    if !superseded.is_file() {
        return;
    }
    eprintln!(
        "{} {} looks superseded by {} — this build names the artifact \
         `.{extension}` and never rewrites the old spelling, so a launcher \
         still naming the `.{retired_extension}` file runs a stale \
         generation. Remove it, and point `node`/Dockerfile/process-manager \
         entries at {}",
        paint::warning_prefix(),
        superseded.display(),
        output.display(),
        output.display(),
    );
}

/// The platform of a leg selected to be *run* under `node`. `select_node_entry`
/// filters to `Platform::Node`, so every launch path below is Node by
/// construction; naming it keeps those paths reading through
/// [`artifact_path`] rather than hardcoding an extension.
const NODE_LEG: Platform = Platform::Node {
    version: vilan_core::target::NODE_LTS,
};

/// Rejects two build units sharing a name — their `dist/` outputs would
/// silently overwrite each other. (`none` members emit nothing, so they can't
/// collide.)
///
/// The check keys on the NAME, not on the emitted bundle path, and that stays
/// right now that the extension is the platform's: two same-named units on the
/// same platform overwrite the bundle outright, and two on *different*
/// platforms still overwrite everything keyed by the bare name beside it — the
/// `<name>.css` sidecar and the `<name>.chunks.json` manifest. So the message
/// names `dist/<name>.*` rather than one extension.
fn reject_output_collisions(members: &[(Unit, Platform)]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for (unit, platform) in members {
        if platform.is_none() {
            continue;
        }
        if !seen.insert(unit.name.as_str()) {
            return Err(format!(
                "two build units are both named `{}`, so their outputs would \
                 collide at dist/{}.*; rename one (the package name or the \
                 `[entry.<name>]`)",
                unit.name, unit.name
            ));
        }
    }
    Ok(())
}

/// Resolves the project rooted at `directory` from its `vilan.toml`. A `[package]`
/// is a single package (`entry` resolves against `root`; `target` is the default),
/// unless it declares `[entry.<name>]` sections — then it lowers onto a workspace
/// with one member per entry. A `[project]` is a workspace — each member builds
/// for its own platform (and may itself declare entries).
fn project_from_manifest(directory: &Path) -> Result<Project, String> {
    let manifest = read_manifest(directory)?;
    let options = manifest
        .build_options()
        .map_err(|error| format!("invalid {}/vilan.toml: {error}", directory.display()))?;

    // A workspace: each `[project] packages` member is built for its own platform.
    if let Some(project) = &manifest.project {
        let mut members = Vec::new();
        for member_path in &project.packages {
            let member_dir = directory.join(member_path);
            let member_manifest = read_manifest(&member_dir)?;
            // A `[library]` member is built only as a dependency of the apps that
            // import it, not on its own — skip it here. Only `[package]` (app)
            // members are buildable units.
            let Some(package) = member_manifest.package.as_ref() else {
                if member_manifest.library.is_some() {
                    continue;
                }
                return Err(format!(
                    "workspace member `{}` is not a `[package]` or `[library]`",
                    member_dir.display()
                ));
            };
            let member_options = member_manifest
                .build_options()
                .map_err(|error| format!("invalid {}/vilan.toml: {error}", member_dir.display()))?;
            members.extend(package_units(
                &member_dir,
                package,
                &member_manifest,
                member_options,
            ));
        }
        reject_output_collisions(&members)?;
        return Ok(Project::Workspace {
            root: directory.to_path_buf(),
            members,
            default_entry: DefaultEntry::new("[project] default-entry", manifest.default_entry()),
            hooks: BuildHooks::from_manifest(directory, &manifest),
        });
    }

    // A standalone `[library]` addressed directly: not a buildable app, but its
    // platform contract is checkable. (`[library]` workspace *members* are handled
    // above — skipped as build units; this is a library directory on its own.)
    if let Some(library) = &manifest.library {
        return Ok(Project::Library {
            dir: directory.to_path_buf(),
            name: library.name.clone().unwrap_or_default(),
        });
    }

    // A single package. `validate` guarantees one of the three sections is present,
    // and the others are ruled out above.
    let package = manifest.package.as_ref().expect("validated package");

    // `[entry.<name>]` sections: the single-package full-stack form
    // (proposal/platform-coloring.md §4.2). Lowers onto the same workspace
    // orchestration as a `[project]` — every entry builds to `dist/<name>`,
    // `run` picks the one node entry, `check` checks them all.
    if !manifest.entries.is_empty() {
        let members = package_units(directory, package, &manifest, options);
        reject_output_collisions(&members)?;
        return Ok(Project::Workspace {
            root: directory.to_path_buf(),
            members,
            default_entry: DefaultEntry::new("[package] default-entry", manifest.default_entry()),
            hooks: BuildHooks::from_manifest(directory, &manifest),
        });
    }

    Ok(Project::Single {
        unit: unit_from_package(directory, package, options),
        platform: package.resolved_target(),
        // A package addressed as a DIRECTORY builds its own entry: one leg and
        // one color — a file-mode question (E113), and it has no file to ask
        // about.
        shared_platforms: Vec::new(),
        hooks: BuildHooks::from_manifest(directory, &manifest),
    })
}

/// Resolves a unit's dependency workspace. A unit with no manifest (a bare file)
/// has no dependencies. Delegates to the shared `vilan_core::manifest::resolve_workspace`
/// so the CLI and LSP resolve identically. (The build platform isn't needed — the
/// graph is platform-independent; the analyzer reports any cross-platform import.)
///
/// The resolution error's *kind* is the editor's concern (an unfetched git
/// dependency is a warning there); here every failure stops the build, so the
/// message is all that survives.
fn resolve_workspace(unit: &Unit) -> Result<Workspace, String> {
    match &unit.package_dir {
        Some(package_dir) => vilan_core::manifest::resolve_workspace(package_dir, &git_deps())
            .map_err(|error| error.to_string()),
        None => Ok(Workspace::default()),
    }
}

/// The CLI's git-dependency policy: **fetch on a cache miss**. This is a command
/// the user ran to build their project, so materializing a declared dependency
/// is the work they asked for — and it is the only thing in the toolchain that
/// reaches the network, still never passively (no build, no fetch).
///
/// The status line goes to **stderr**: it is progress, and stdout has to stay
/// byte-clean for `build --stdout`. Dim like `vilan upgrade`'s download line,
/// TTY-gated by `paint` like every other status line.
fn git_deps() -> vilan_core::git_dep::GitDeps {
    vilan_core::git_dep::GitDeps::fetching(vilan_embedded_std::default_git_dep_root())
        .reporting(|message| eprintln!("{}", paint::err(paint::Style::DIM, message)))
}

/// The same cache root, read-only — for a resolution the build only *asks a
/// question* of, and which must therefore not fetch. The compile's own
/// resolution ([`git_deps`]) is what materializes a dependency; a reporting
/// pass that fetched would move the network ahead of the `[build]` hooks and
/// change what the build does in order to describe it.
fn git_deps_cached() -> vilan_core::git_dep::GitDeps {
    vilan_core::git_dep::GitDeps::cache_only(vilan_embedded_std::default_git_dep_root())
}

/// Resolves a unit's workspace and compiles its entry for `platform`, returning the
/// emitted JavaScript (or a failure code after reporting).
/// What a compile is FOR — the one thing that changes how a file which did not
/// parse cleanly is treated (`editing-dx.md` S6/§13.1).
///
/// `Emit` keeps the historical contract: a file whose parse was not clean is not
/// analyzed at all, so a broken build reports its parse errors and nothing else.
/// `Check` analyzes the salvaged tree instead — the same tree, on the same parse,
/// that the language server has analyzed since the H6 cutover — so a syntax error
/// in one statement stops hiding the type errors everywhere else in the file
/// (§2.2 mechanism 1, measured as P29). Neither goal ever emits JavaScript from a
/// recovered tree.
///
/// `CheckModule` is `Check` for a file that is **not** a program entry — a
/// module of its package, addressed by path. It skips the emission walk, whose
/// one failure is the missing `main` a module has no business declaring: file
/// mode compiled every path it was given as if it were the program, so
/// `vilan check src/interact.vl` on a perfectly good module answered "Cannot
/// execute program without a main function" (found fixing E113). An ENTRY
/// keeps `Check`, so a `vilan check .` whose entry really has lost its `main`
/// still says so rather than going green over a build that cannot succeed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CompileGoal {
    Emit,
    Check,
    CheckModule,
}

impl CompileGoal {
    /// Whether this goal analyzes a tree recovered from syntax errors (both
    /// checking goals do; emission never does).
    fn analyzes_recovered_trees(self) -> bool {
        matches!(self, CompileGoal::Check | CompileGoal::CheckModule)
    }

    /// Whether this goal runs the emission walk. `CheckModule` does not: a
    /// module is not a program, and emission's only diagnostic says so.
    fn emits(self) -> bool {
        !matches!(self, CompileGoal::CheckModule)
    }
}

/// What one compile produced, for the caller that writes it out. A tuple grew
/// a fourth member with `asset::bundle` (kolt.local 029) and stopped reading;
/// naming the members also makes the two paths that ignore all but the
/// JavaScript say so by not mentioning the rest.
struct Compiled {
    javascript: String,
    /// The contributions `asset::emit` / `asset::emit_keyed` accumulated —
    /// `write_assets` deduplicates and orders them into `<output>.<kind>`.
    assets: Vec<vilan_core::const_eval::EmittedAsset>,
    /// The files `asset::bundle` registered: (resolved source, the name it
    /// takes in the output directory). `write_bundled` copies them.
    bundled: Vec<(PathBuf, String)>,
    /// Each source this leg was compiled from, with the content hash it was
    /// compiled at — what the watch loop re-hashes to decide a per-leg skip.
    sources: Vec<(PathBuf, u64)>,
    /// The const channel's provenance, resolved to `file:line` — what
    /// `vilan build --explain` prints (G11). EMPTY unless the flag asked for
    /// it: resolving a line means re-reading the source it is counted in, and
    /// a build nobody asked to explain must not pay for a report it will not
    /// print.
    explain: Vec<explain::Fact>,
}

fn compile_unit(
    unit: &Unit,
    platform: Platform,
    goal: CompileGoal,
    emit_debug: bool,
    hmr: bool,
    overlay: Option<&mut String>,
    // The artifact stem this leg writes under, and where its route chunks land,
    // when it declared `split = true` (`bundle-splitting.md` S2). The stem comes
    // from the caller because it is the OUTPUT's name (`dist/<leg>`, or the
    // entry file's own stem for a lone package), not the unit's. A caller that
    // passes `None` — `check`, `run`'s temp build, every watch round — compiles
    // the leg as one file, which is what keeps splitting a `build` artifact
    // decision and nothing else.
    chunks: Option<(&str, &mut Vec<EmittedChunk>)>,
) -> Result<Compiled, ExitCode> {
    let mut workspace = match resolve_workspace(unit) {
        Ok(workspace) => workspace,
        Err(message) => {
            eprintln!("{} {message}", paint::error_prefix());
            return Err(ExitCode::FAILURE);
        }
    };
    // E119: why THIS compile is coloured the way it is, for the diagnostics that
    // follow from the colour. Keyed on the platform, because a shared module is
    // compiled once per leg and each leg has its own answer.
    workspace.platform_reason = unit
        .platform_reasons
        .iter()
        .find(|(colored, _)| *colored == platform)
        .map(|(_, reason)| reason.clone());
    // B239: whether the file this compile was pointed at is a program the
    // package declares, or one of its modules addressed by path. Threaded on
    // the same context and for the same reason as the line above — a fact about
    // THIS compile that only the front end, which read the manifest, can know.
    //
    // B240: file mode carries the manifest's DECLARED-ENTRY set with it, so the
    // analysis can see that a SIBLING is a program — `views.vl` importing
    // `pkg::client` is refused here exactly as `vilan check .`'s `client` leg
    // refuses it.
    workspace.entry_mode = unit.entry_mode.clone();
    // HMR instrumentation is opt-in per compile (an HMR-active `run --watch`,
    // browser legs only) — every other caller passes `false`, so `build`/`run`/
    // `check` output stays byte-identical.
    let mut options = unit.options;
    options.hmr = hmr;
    let split = chunks.filter(|_| unit.split);
    compile_to_js(
        &unit.entry,
        &unit.pkg_root,
        platform,
        goal,
        &options,
        &workspace,
        emit_debug,
        overlay,
        split,
    )
}

/// Builds a lone package / bare file, writing `<entry>.mjs` on a process leg
/// and `<entry>.js` on the browser (or printing to stdout).
fn build_single(unit: &Unit, stdout: bool, platform: Platform, emit_debug: bool) -> RoundOutcome {
    let mut chunks = Vec::new();
    // A lone package writes `<entry>.<ext>` beside its source, so the entry file's
    // own stem is what its chunks are named after.
    let leg = unit
        .entry
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let compiled = match compile_unit(
        unit,
        platform,
        CompileGoal::Emit,
        emit_debug,
        false,
        None,
        Some((leg.as_str(), &mut chunks)),
    ) {
        Ok(compiled) => compiled,
        // The `Err` is the whole verdict: every fallible step here answers with
        // `ExitCode::FAILURE`, so the discarded code says nothing more.
        Err(_) => return RoundOutcome::Failed,
    };
    if stdout {
        // `--stdout` is one stream, so it carries the eager bundle. A split
        // build's chunks are files by construction; nothing can pipe them.
        // Bundled resources are files by construction too, so `--stdout`
        // carries none of them either — it prints a bundle, not a build.
        print!("{}", compiled.javascript);
        return RoundOutcome::Succeeded;
    }
    // Before the writers, which record the files this leg's facts explain.
    explain::leg_facts(&leg, compiled.explain);
    let output_path = unit.entry.with_extension(platform.script_extension());
    let styles = write_assets(&output_path, &compiled.assets);
    let directory = output_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let assets = match write_bundled(
        &directory,
        &compiled.bundled,
        &leg,
        &[LegNamespace::of(&leg, platform)],
        &mut BTreeMap::new(),
    ) {
        Ok(assets) => assets,
        Err(_) => return RoundOutcome::Failed,
    };
    if write_chunks(
        &output_path,
        &chunks,
        styles.as_deref(),
        &assets,
        matches!(platform, Platform::Browser),
    )
    .is_err()
    {
        return RoundOutcome::Failed;
    }
    match fs::write(&output_path, compiled.javascript) {
        Ok(()) => {
            println!(
                "{} {} -> {}",
                paint::out(paint::Style::GREEN, "Compiled"),
                unit.entry.display(),
                paint::out(paint::Style::BOLD, &output_path.display().to_string())
            );
            warn_superseded_sibling(&output_path);
            explain::bundle(output_path.clone(), &leg);
            // The build is complete, so the report is complete. A build that
            // failed has not written the tree it would be explaining, and its
            // diagnostics are the account it owes.
            explain::print();
            RoundOutcome::Succeeded
        }
        Err(error) => {
            eprintln!(
                "{} cannot write {}: {error}",
                paint::error_prefix(),
                output_path.display()
            );
            RoundOutcome::Failed
        }
    }
}

/// Type-checks a lone package / bare file, writing no output. `goal` is
/// `CompileGoal::CheckModule` when the addressed file is a module of its
/// package rather than one of its entries (E113).
///
/// `platforms` is usually one. It is several for a module SHARED between the
/// legs of a multi-entry package: the build compiles such a file once per leg
/// and it must type-check under every one of them, so the check reports each
/// leg's diagnostics and a clean verdict means clean everywhere (E113). One
/// verdict line either way — the file is the subject, not the number of colors
/// it took to clear it.
fn check_single(
    unit: &Unit,
    platforms: &[Platform],
    emit_debug: bool,
    goal: CompileGoal,
) -> RoundOutcome {
    // The same one-report-per-round ledger `check_workspace` arms, for the same
    // reason (B182): several colors over ONE file is several analyses of one
    // source tree, and a refusal that holds under every leg is one refusal. A
    // diagnostic only ONE color raises still renders — the key carries the
    // reason, so two colors' answers are two errors.
    let _round = RoundReports::arm();
    let mut ok = true;
    for platform in platforms {
        ok &= compile_unit(unit, *platform, goal, emit_debug, false, None, None).is_ok();
    }
    if !ok {
        return RoundOutcome::Failed;
    }
    println!(
        "{}: {}",
        unit.entry.display(),
        paint::out(paint::Style::GREEN, "no errors")
    );
    RoundOutcome::Succeeded
}

/// Builds and runs a lone package's entry with Node, forwarding `args`.
fn run_single(unit: &Unit, args: &[String]) -> ExitCode {
    let platform = Platform::default();
    let compiled = match compile_unit(unit, platform, CompileGoal::Emit, false, false, None, None) {
        Ok(compiled) => compiled,
        Err(code) => return code,
    };
    // Const-eval assets (the CSS sidecar &c.) belong beside the *canonical* build
    // output — `<entry>.css`, where `build` writes them and a served page reads
    // them — not beside the temp script `run_node_script` hands Node, which the
    // program never reads. Same helper and placement as `build_single`, so `run`
    // keeps the on-disk sidecar fresh (const-eval.md §3; hmr.md §11 S0).
    let output_path = unit.entry.with_extension(platform.script_extension());
    write_assets(&output_path, &compiled.assets);
    // Bundled resources land beside that same canonical output, for the same
    // reason the sidecar does: `run` keeps the on-disk build fresh so a served
    // program reads what this round produced (kolt.local 029).
    if write_bundled(
        output_path.parent().unwrap_or(Path::new(".")),
        &compiled.bundled,
        // `run` never explains — `--explain` is a `vilan build` flag — so this
        // reaches the report never and the bundled record always: the leg name
        // `write_assets` just pruned this directory's kind files under.
        &leg_name(&output_path),
        &[],
        &mut BTreeMap::new(),
    )
    .is_err()
    {
        return ExitCode::FAILURE;
    }
    run_node_script(&compiled.javascript, args)
}

/// Builds every host (non-`none`) member of a workspace into
/// `<root>/dist/<name>.<ext>` (`.mjs` on a process leg, `.js` on the browser)
/// — a `none` member is a pure library, compiled only as a dependency of a host.
/// Members build in declaration order (the client before the server, so the
/// server's `dist/client.js` exists). `--platform`/`--stdout` don't apply.
fn build_workspace(
    root: &Path,
    members: &[(Unit, Platform)],
    debug: bool,
    watch_state: Option<&mut BuildWatchState>,
) -> RoundOutcome {
    // A borrow of the state has to survive the call, and the failure arm has
    // to write to it — so take the flag update through a raw re-borrow rather
    // than moving the option in twice.
    let mut state = watch_state;
    let outcome = build_workspace_artifacts(
        root,
        members,
        debug,
        Emission::AsDeclared,
        state.as_deref_mut(),
    );
    match outcome {
        Ok(()) => {
            if let Some(state) = state {
                state.failed = false;
            }
            // Every leg is written, so the report is complete — see
            // `build_single` for why only a successful build prints one.
            explain::print();
            RoundOutcome::Succeeded
        }
        Err(_) => {
            // A failed round's `dist/` is not a baseline: the next round
            // recompiles every leg (`hmr::round_forces_full`).
            if let Some(state) = state {
                state.failed = true;
            }
            RoundOutcome::Failed
        }
    }
}

/// What one `vilan build --watch` round remembers for the next one, so a leg
/// the edit did not reach is REUSED rather than recompiled (backlog M22).
///
/// `run --watch` has had this since E12 half b; `build --watch` never did, and
/// the measured consequence on kolt was that a one-character edit in
/// `views.vl` — a module only the CLIENT leg loads — recompiled client, probe
/// AND server every round. The state is deliberately the same three fields the
/// HMR round keeps, and the decision is made by the same two functions
/// (`hmr::round_forces_full`, `hmr::leg_is_current`): two watch loops that
/// answer "is this leg current?" two different ways is exactly the drift this
/// tree refuses elsewhere.
///
/// Only `--watch` builds carry one. A one-shot `vilan build` passes `None` and
/// is byte-for-byte the build it always was.
#[derive(Default)]
struct BuildWatchState {
    /// Per leg, in `members` order: what the last round compiled it from and
    /// what it bundled. Empty on the first round, which is one of the guards
    /// that forces a full one.
    legs: Vec<BuildWatchLeg>,
    /// Every `vilan.toml` in the tree, hashed: a manifest change can alter
    /// output without touching a `.vl` source, so it forces a full round.
    manifest: Option<u64>,
    /// The previous round failed, so nothing it left in `dist/` is trustworthy.
    failed: bool,
}

/// One leg's record in [`BuildWatchState`].
struct BuildWatchLeg {
    name: String,
    /// Each source the leg was compiled from, mapped to the content hash it
    /// was compiled at — re-hashed, never re-stat'ed (the E12 rule).
    sources: BTreeMap<PathBuf, u64>,
    /// What `const asset::bundle` registered for this leg, kept so a SKIPPED
    /// leg still occupies its output names in the cross-leg collision check
    /// below: two legs bundling two different files to one name is an error
    /// whether or not this round recompiled both of them.
    bundled: Vec<(PathBuf, String)>,
}

/// Whether a build honours a browser leg's `[entry.<name>] split`
/// (`bundle-splitting.md` §4, §S4 item 6). `vilan build` does; every `run` form
/// does not, watched or not.
///
/// The doctrine is that single-file emission is first-class forever and
/// splitting is a BUILD optimization: HMR classifies by whole-bundle byte diff
/// and swaps a whole blob, `run --watch` is the only way to develop, and a
/// refusal would mean a project that ships split could not be developed without
/// editing its manifest. So `run` emits one file per leg, says so once, and the
/// leg's chunk namespace is swept clean by [`write_chunks`] so `dist/` never
/// describes a build that is no longer there.
#[derive(Clone, Copy, PartialEq)]
enum Emission {
    AsDeclared,
    WholeBundles,
}

/// Says, once per process, that a `run` is passing over a leg's `split`. Once,
/// not once per watch round: a watcher lives for hours.
fn note_split_ignored(unit: &Unit) {
    static NOTED: std::sync::Once = std::sync::Once::new();
    if !unit.split {
        return;
    }
    NOTED.call_once(|| {
        eprintln!(
            "{}",
            paint::err(
                paint::Style::DIM,
                &format!(
                    "run: `{}` emits as one file — `split` is a `vilan build` optimization \
                     (the dev loop hot-swaps whole bundles). `vilan build` writes its route chunks.",
                    unit.name
                )
            )
        );
    });
}

fn build_workspace_artifacts(
    root: &Path,
    members: &[(Unit, Platform)],
    debug: bool,
    emission: Emission,
    // `Some` only under `vilan build --watch` (backlog M22): the previous
    // round's per-leg record, which decides which legs this round may reuse.
    // Replaced with this round's record on the way out.
    watch_state: Option<&mut BuildWatchState>,
) -> Result<(), ExitCode> {
    let dist = root.join("dist");
    if let Err(error) = fs::create_dir_all(&dist) {
        eprintln!(
            "{} cannot create {}: {error}",
            paint::error_prefix(),
            dist.display()
        );
        return Err(ExitCode::FAILURE);
    }
    // Every leg's namespace, not just the one being written: `dist/` is one
    // directory, so a client leg bundling `server.mjs` would clobber the server
    // exactly as it would clobber its own bundle.
    let reserved: Vec<LegNamespace> = members
        .iter()
        .filter(|(_, platform)| !platform.is_none())
        .map(|(unit, platform)| LegNamespace::of(&unit.name, *platform))
        .collect();
    // Shared across legs for the same reason: two legs bundling two different
    // files to one output name is one `dist/` asked to serve two files on one
    // url, which `asset::bundle_as` made expressible (const-eval.md §3.1).
    let mut bundled_names: BTreeMap<String, PathBuf> = BTreeMap::new();
    // M22 — whether this round may SKIP a leg at all. Same decision, same two
    // functions and the same safety cases as the HMR round's: reuse is by
    // CONTENT (every source the leg's artifact was compiled from re-hashes to
    // what it was compiled with), never by mtime — the watcher's scan only
    // TRIGGERS rounds. `--explain` forces a full round: the report is a
    // statement about what THIS build wrote, and a skipped leg has no facts to
    // contribute, so reusing one would silently shorten the report rather than
    // speed it up.
    //
    // **Whether**, not *which*: B203. The per-leg question is asked at each
    // leg's own turn, below, because one leg's sources can include another
    // leg's artifact.
    let mut watch_state = watch_state;
    // The tree walk it costs is paid once per round, whatever it decides.
    let manifest = watch_state.is_some().then(|| manifest_fingerprint(root));
    let may_reuse = match watch_state.as_deref() {
        Some(state) => {
            !explain::asked()
                && !hmr::round_forces_full(
                    state.legs.is_empty(),
                    state.failed,
                    state
                        .manifest
                        .is_some_and(|previous| Some(previous) != manifest),
                )
        }
        None => false,
    };
    // B203 — a producer leg compiles before the leg that reads its artifact.
    let schedule = {
        let scheduled: Vec<ScheduledLeg> = members
            .iter()
            .map(|(unit, platform)| {
                let previous = state_leg(watch_state.as_deref(), &unit.name);
                ScheduledLeg {
                    name: &unit.name,
                    extension: platform.script_extension(),
                    bundled: previous.map(|leg| leg.bundled.as_slice()).unwrap_or(&[]),
                    sources: previous.map(|leg| &leg.sources),
                }
            })
            .collect();
        leg_schedule(&dist, &scheduled)
    };
    // Which legs this round actually recompiled, in schedule order — read by
    // `downstream_of_a_recompile` at each later leg's turn.
    let mut recompiled: BTreeSet<usize> = BTreeSet::new();
    if let Some(state) = watch_state.as_deref_mut() {
        state.manifest = manifest;
    }
    // This round's record, built as the legs are compiled (or carried over) and
    // kept in DECLARATION order however the round chose to compile them: the
    // record is a statement about the workspace, not about one round's schedule.
    let mut recorded: Vec<Option<BuildWatchLeg>> = (0..members.len()).map(|_| None).collect();
    for index in schedule.order.clone() {
        let (unit, platform) = &members[index];
        if platform.is_none() {
            continue;
        }
        // B203 — asked HERE, at this leg's turn, and not for every leg before
        // the round began. A leg's recorded sources can include another leg's
        // `dist/` artifact (a server leg bundling the client's bundle), and a
        // freshness question asked before that producer compiled is answered
        // against the artifact it is about to overwrite: the consumer was
        // judged fresh, skipped, and left holding bytes that no longer exist.
        // The order above puts the producer first; asking at this turn is what
        // makes the answer describe the `dist/` this round will ship.
        let fresh = may_reuse
            && !schedule.downstream_of_a_recompile(index, &recompiled)
            && state_leg(watch_state.as_deref(), &unit.name)
                .is_some_and(|leg| hmr::leg_is_current(&leg.sources, current_source_hash));
        if fresh {
            // Reuse: the leg's artifact in `dist/` was compiled from exactly
            // these bytes, so a recompile would rewrite the file it already
            // holds. Its bundled names still have to occupy the collision map
            // — the other legs' copies are checked against them this round
            // just as they were last round — and they are already on disk, so
            // no copy is repeated.
            let previous = state_leg(watch_state.as_deref(), &unit.name)
                .expect("`fresh` is decided off the recorded leg");
            for (source, name) in &previous.bundled {
                bundled_names.insert(name.clone(), source.clone());
            }
            let output = artifact_path(&dist, &unit.name, *platform);
            println!(
                "{} {} -> {}",
                paint::out(paint::Style::CYAN, "Fresh"),
                unit.entry.display(),
                paint::out(paint::Style::BOLD, &output.display().to_string())
            );
            recorded[index] = Some(BuildWatchLeg {
                name: previous.name.clone(),
                sources: previous.sources.clone(),
                bundled: previous.bundled.clone(),
            });
            continue;
        }
        recompiled.insert(index);
        if emission == Emission::WholeBundles {
            note_split_ignored(unit);
        }
        let mut chunks = Vec::new();
        let sink = (emission == Emission::AsDeclared).then_some((unit.name.as_str(), &mut chunks));
        let mut compiled =
            compile_unit(unit, *platform, CompileGoal::Emit, debug, false, None, sink)?;
        // What the NEXT round re-hashes to decide this leg's skip (M22).
        // Recorded whether or not a watch is running: the cost is a clone of
        // the loaded-file list, and a state to write it into is what makes it
        // a watch.
        recorded[index] = Some(BuildWatchLeg {
            name: unit.name.clone(),
            sources: compiled.sources.iter().cloned().collect(),
            bundled: compiled.bundled.clone(),
        });
        // Before the writers, which record the files this leg's facts explain.
        explain::leg_facts(&unit.name, std::mem::take(&mut compiled.explain));
        let output = artifact_path(&dist, &unit.name, *platform);
        let styles = write_assets(&output, &compiled.assets);
        let assets = write_bundled(
            &dist,
            &compiled.bundled,
            &unit.name,
            &reserved,
            &mut bundled_names,
        )?;
        // Unconditional: this is also where a previous build's chunks are swept
        // when this one wrote none, and where a browser leg's build manifest is
        // written whether it split or not (`fullstack-dx.md` §10.3).
        write_chunks(
            &output,
            &chunks,
            styles.as_deref(),
            &assets,
            matches!(platform, Platform::Browser),
        )?;
        if let Err(error) = fs::write(&output, compiled.javascript) {
            eprintln!(
                "{} cannot write {}: {error}",
                paint::error_prefix(),
                output.display()
            );
            return Err(ExitCode::FAILURE);
        }
        println!(
            "{} {} -> {}",
            paint::out(paint::Style::GREEN, "Compiled"),
            unit.entry.display(),
            paint::out(paint::Style::BOLD, &output.display().to_string())
        );
        warn_superseded_sibling(&output);
        explain::bundle(output, &unit.name);
    }
    if let Some(state) = watch_state {
        state.legs = recorded.into_iter().flatten().collect();
    }
    Ok(())
}

/// A leg's record from the previous `build --watch` round, by name.
fn state_leg<'state>(
    state: Option<&'state BuildWatchState>,
    name: &str,
) -> Option<&'state BuildWatchLeg> {
    state?.legs.iter().find(|leg| leg.name == name)
}

/// One leg as the round's schedule sees it (B203): what it is called, what it
/// writes into `dist/`, and what the PREVIOUS round compiled it from.
///
/// Both watch loops build this from their own state — `build --watch` from
/// [`BuildWatchState`], the HMR round from [`WatchState`] — because the
/// question "which leg reads which leg's artifact" is one question and must not
/// be answered two ways.
struct ScheduledLeg<'round> {
    name: &'round str,
    /// The bundle's extension, which is half of what [`LegNamespace`] needs to
    /// say what a leg's output names are.
    extension: &'static str,
    /// The names this leg's `const asset::bundle` copies took in `dist/` last
    /// round. Not derivable from anything: the leg's own sources chose them.
    bundled: &'round [(PathBuf, String)],
    /// What the leg was compiled from last round, or `None` for a leg with no
    /// record (a `none` platform, or a first round).
    sources: Option<&'round BTreeMap<PathBuf, u64>>,
}

/// Whether `path` is a file `leg` WRITES into `dist/` — the edge the round's
/// leg ordering is built from (B203).
///
/// Two kinds of output, and they come from two places because they are known
/// two different ways:
///
/// * the leg's **own namespace** — its bundle, its style sidecar, its build
///   manifest, its whole route-chunk pattern. [`LegNamespace::claims`] already
///   answers exactly this question, for the cross-leg collision fence, and
///   reusing it is what keeps one leg's idea of what it owns from drifting
///   from another's.
/// * the leg's **bundled copies**, whose names the leg's sources chose. Those
///   are not a pattern and cannot be derived; they are read off the previous
///   round's record, which is the same place the dependency itself is read
///   from.
///
/// Both sides are resolved, because the recorded source path came from the
/// const channel's own resolution and this one is built by joining — the seam
/// `util::canonical_path` exists for. `canonical_path_of_unwritten` on the
/// subject: a `dist/` entry a round has not written yet still has to compare
/// equal to the same name spelled through a resolved root (B198's rule).
fn leg_writes(dist: &Path, leg: &ScheduledLeg, path: &Path) -> bool {
    let path = vilan_core::util::canonical_path_of_unwritten(path);
    let Ok(relative) = path.strip_prefix(vilan_core::util::canonical_path(dist)) else {
        return false;
    };
    let Some(relative) = relative.to_str() else {
        return false;
    };
    LegNamespace {
        leg: leg.name.to_string(),
        extension: leg.extension,
    }
    .claims(relative)
    .is_some()
        || leg
            .bundled
            .iter()
            .any(|(_, bundled)| bundled.as_str() == relative)
}

/// How a round compiles its legs, and which of them may be reused (B203).
///
/// Both watch loops decided "fresh" for every leg BEFORE any leg compiled, off
/// hashes recorded in the previous round. When one leg's recorded sources
/// include another leg's `dist/` artifact — a server leg bundling the client's
/// bundle — the consumer was measured against the producer's OLD artifact,
/// judged fresh, and skipped; the producer then rewrote that file, and the
/// consumer's own artifact went on embedding bytes that no longer exist until
/// some unrelated edit happened to reach it.
///
/// The schedule answers both halves of the cure:
///
/// * [`order`](Self::order) — the producer compiles FIRST, so `dist/` holds
///   this round's bytes by the time anything downstream is asked about it, and
///   the report reads in dependency order;
/// * [`reads_the_artifacts_of`](Self::reads_the_artifacts_of) — from which
///   [`downstream_of_a_recompile`](Self::downstream_of_a_recompile) says, at
///   each leg's own turn, whether a leg it reads has already recompiled in
///   THIS round. That is the half the HMR round needs on its own: it writes
///   `dist/` after the whole compile loop, so a re-hash against the disk cannot
///   see the round's own work however the legs are ordered.
struct LegSchedule {
    order: Vec<usize>,
    reads_the_artifacts_of: Vec<BTreeSet<usize>>,
}

impl LegSchedule {
    /// Whether a leg already-recompiled this round produces something `leg`
    /// reads. Conservative on purpose: a producer that recompiled to
    /// byte-identical output still costs its consumer a recompile, because the
    /// alternative — comparing the bytes the round is about to write against
    /// the ones it has not written yet — is exactly the reasoning-ahead that
    /// caused this bug. A spurious recompile of one leg is the cheap direction;
    /// a stale artifact that survives every later round is the expensive one.
    fn downstream_of_a_recompile(&self, leg: usize, recompiled: &BTreeSet<usize>) -> bool {
        !self.reads_the_artifacts_of[leg].is_disjoint(recompiled)
    }
}

/// Builds a round's [`LegSchedule`] from the previous round's records.
///
/// The edges come from the PREVIOUS round's record, which is the only place
/// they can come from — a leg's sources are what compiling it reveals. That is
/// not a gap in the fix but the same boundary the skip decision has: a round
/// with no record forces a full recompile of every leg
/// ([`hmr::round_forces_full`]), so the round that cannot know the edges is
/// also the round in which no leg is reused and the order is a schedule rather
/// than a correctness question. From the second round on, the record is there.
///
/// Ties keep declaration order ([`hmr::legs_in_artifact_order`] is stable), so a
/// workspace whose legs read nothing of each other's compiles exactly as it
/// always did, and `dist/` is written in the order the manifest lists.
fn leg_schedule(dist: &Path, legs: &[ScheduledLeg]) -> LegSchedule {
    let reads_the_artifacts_of: Vec<BTreeSet<usize>> = legs
        .iter()
        .map(|consumer| {
            let Some(sources) = consumer.sources else {
                return BTreeSet::new();
            };
            legs.iter()
                .enumerate()
                .filter(|(_, producer)| {
                    producer.name != consumer.name
                        && sources
                            .keys()
                            .any(|source| leg_writes(dist, producer, source))
                })
                .map(|(index, _)| index)
                .collect()
        })
        .collect();
    LegSchedule {
        order: hmr::legs_in_artifact_order(&reads_the_artifacts_of),
        reads_the_artifacts_of,
    }
}

/// A watched source's content hash RIGHT NOW, read the way the compiler reads
/// it (BOM dropped, `windows-support.md` §2) so the compare is against the
/// text the compiler consumed. A recorded input that is a DIRECTORY re-hashes
/// as its listing (`asset::read_dir`, const-eval.md §3.1), so a file appearing
/// or vanishing in a listed tree fails the compare. `None` — deleted,
/// unreadable — disqualifies the skip by construction.
///
/// The same reader the HMR round uses, for the same reason it uses it.
fn current_source_hash(path: &Path) -> Option<u64> {
    if path.is_dir() {
        return vilan_core::const_eval::directory_input_hash(path);
    }
    vilan_core::util::read_source(path)
        .ok()
        .map(|text| vilan_core::content_hash(&text))
}

/// Type-checks every member of a workspace (each for its own platform; a `none`
/// library against the base layer) — and every ENTRY of a multi-entry package,
/// which reaches here as its own member.
///
/// One round, one report per distinct error (B182). The members share a source
/// tree: a module several legs reach is analyzed once per leg and yields the
/// same diagnostics each time, and reading the same refusal three times says
/// nothing the first did not. [`RoundReports`] scopes the ledger to this loop —
/// each `check` starts with an empty one, so a `--watch` round always reports.
fn check_workspace(members: &[(Unit, Platform)], debug: bool) -> RoundOutcome {
    let _round = RoundReports::arm();
    let mut ok = true;
    for (unit, platform) in members {
        ok &= compile_unit(
            unit,
            *platform,
            CompileGoal::Check,
            debug,
            false,
            None,
            None,
        )
        .is_ok();
    }
    if ok {
        RoundOutcome::Succeeded
    } else {
        RoundOutcome::Failed
    }
}

/// Selects the ONE Node leg a `run` executes from a workspace's members (A15).
/// The order of authority: an explicit `--entry <name>`, then the manifest's
/// `default-entry`, then a lone Node leg; a browser-only workspace has none
/// (`Ok(None)`). The non-selected Node legs are still compiled by the caller —
/// they are part of the workspace — but never launched. `Err` when the choice is
/// ambiguous (2+ Node legs, nothing designating one) or when a designation names
/// something that isn't a runnable Node leg; the message lists the candidates
/// and names the flag or the manifest key that made the choice.
fn select_node_entry<'members>(
    members: &'members [(Unit, Platform)],
    entry: Option<&str>,
    default_entry: &DefaultEntry,
) -> Result<Option<&'members Unit>, String> {
    let node_members: Vec<&Unit> = members
        .iter()
        .filter(|(_, platform)| matches!(platform, Platform::Node { .. }))
        .map(|(unit, _)| unit)
        .collect();
    let find = |name: &str| node_members.iter().find(|unit| unit.name == name).copied();
    if let Some(name) = entry {
        // The flag wins over the manifest — that is what a flag is for.
        return find(name).map(Some).ok_or_else(|| {
            format!(
                "no `node` package named `{name}` to run{}",
                candidate_tail(&node_members)
            )
        });
    }
    if let Some(name) = &default_entry.name {
        return find(name).map(Some).ok_or_else(|| {
            format!(
                "`{} = \"{name}\"` names no `node` package to run{}",
                default_entry.key,
                candidate_tail(&node_members)
            )
        });
    }
    match node_members.as_slice() {
        [] => Ok(None),
        [unit] => Ok(Some(unit)),
        _ => Err(ambiguous_node_entry(&node_members, default_entry.key)),
    }
}

/// The candidate package names, in workspace declaration order, for an entry
/// error message: `server, probe`.
fn node_entry_candidates(node_members: &[&Unit]) -> String {
    node_members
        .iter()
        .map(|unit| unit.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The message for a `run` on a multi-node workspace with nothing designating a
/// leg (A15): both ways to designate one, the flag for this run and the
/// manifest key for every run.
fn ambiguous_node_entry(node_members: &[&Unit], default_entry_key: &str) -> String {
    format!(
        "this workspace has more than one `node` package to run; pick one with \
         --entry <name>, or designate one for good with `{default_entry_key}` in \
         vilan.toml: {}",
        node_entry_candidates(node_members)
    )
}

/// The tail of a "no such entry" message: the candidates, or the reason there
/// are none. Shared by the `--entry` and `default-entry` failures so a mistyped
/// name reads the same whichever named it.
fn candidate_tail(node_members: &[&Unit]) -> String {
    if node_members.is_empty() {
        " (this workspace runs no `node` package)".to_string()
    } else {
        format!("; candidates: {}", node_entry_candidates(node_members))
    }
}

/// Builds a workspace, then runs its selected Node member (A15) with `node` from
/// the project root (so it can read sibling `dist/` bundles). `args` are forwarded.
fn run_workspace(
    root: &Path,
    members: &[(Unit, Platform)],
    args: &[String],
    entry: Option<&str>,
    default_entry: &DefaultEntry,
) -> ExitCode {
    let server = match select_node_entry(members, entry, default_entry) {
        Ok(Some(unit)) => unit,
        Ok(None) => {
            eprintln!(
                "{} no `node` package in this workspace to run",
                paint::error_prefix()
            );
            return ExitCode::FAILURE;
        }
        Err(message) => {
            eprintln!("{} {message}", paint::error_prefix());
            return ExitCode::FAILURE;
        }
    };
    if let Err(code) = build_workspace_artifacts(root, members, false, Emission::WholeBundles, None)
    {
        return code;
    }
    // Run from the project root so the server reads sibling `dist/` bundles; the script
    // path is relative to that working directory.
    let status = spawn_node(
        &artifact_path(Path::new("dist"), &server.name, NODE_LEG),
        args,
        Some(root),
    )
    .and_then(|mut child| child.wait());
    exit_code_of(status)
}

/// Walks up from `start` for the nearest directory containing a `vilan.toml`.
fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut directory = start;
    loop {
        if directory.join("vilan.toml").is_file() {
            return Some(directory.to_path_buf());
        }
        directory = directory.parent()?;
    }
}

/// Runs the package's `*_test.vl` tests: each is compiled and executed, passing if
/// it exits 0 (a failed `assert` panics -> non-zero). Reports a pass/fail summary
/// and exits non-zero if any test fails.
fn test(path: Option<PathBuf>) -> RoundOutcome {
    let tests = match discover_tests(path) {
        Ok(tests) => tests,
        Err(message) => {
            eprintln!("{} {message}", paint::error_prefix());
            return RoundOutcome::Failed;
        }
    };
    if tests.is_empty() {
        println!(
            "{}",
            paint::out(paint::Style::DIM, "no `*_test.vl` tests found")
        );
        return RoundOutcome::Succeeded;
    }
    println!(
        "running {} test(s)",
        paint::out(paint::Style::BOLD, &tests.len().to_string())
    );
    let mut passed = 0u32;
    let mut failed = 0u32;
    for test in &tests {
        match run_test(test) {
            Ok(()) => {
                passed += 1;
                println!(
                    "  {}    {}",
                    paint::out(paint::Style::GREEN, "ok"),
                    test.display()
                );
            }
            Err(detail) => {
                failed += 1;
                println!(
                    "  {}  {}",
                    paint::out(paint::Style::BOLD_RED, "FAIL"),
                    test.display()
                );
                for line in detail.lines() {
                    println!("        {line}");
                }
            }
        }
    }
    // A zero failure count is muted rather than alarmed in red.
    let failed_style = if failed == 0 {
        paint::Style::DIM
    } else {
        paint::Style::BOLD_RED
    };
    println!(
        "\n{}, {}",
        paint::out(paint::Style::BOLD_GREEN, &format!("{passed} passed")),
        paint::out(failed_style, &format!("{failed} failed"))
    );
    if failed == 0 {
        RoundOutcome::Succeeded
    } else {
        RoundOutcome::Failed
    }
}

/// How one `*_test.vl` compiles: the package source root its `pkg::` imports
/// resolve against, the dependency workspace, and the package's build options.
///
/// A test file is a file **of its package** (the Go-style, alongside-the-source
/// model), so it compiles the way that package compiles: the same manifest
/// discovery from the test's own location, the same `root`, the same
/// dependencies (distribution.md §7's S4 residual — until now `vilan test`
/// compiled against `Workspace::default()`, so a test could import neither a
/// `pkg::` sibling through a declared `root` nor any dependency at all).
///
/// Two boundaries, stated: git dependencies resolve under the **fetching**
/// policy, because running the tests is a build the user asked for — the same
/// policy `build`, `run`, and `check` use. And a test compiles for **`node`**
/// whatever the package's `target` says, because `vilan test` executes the
/// emitted JS with `node`; a browser package's tests are node programs that may
/// import its neutral modules.
///
/// A file with no manifest above it keeps its old context exactly: its own
/// directory as the package root, no dependencies, default options. So does a
/// test sitting at a `[project]` root, which has no sources of its own.
fn test_context(file: &Path) -> Result<(PathBuf, Workspace, BuildOptions), String> {
    let bare = || {
        (
            pkg_root_of(file),
            Workspace::default(),
            BuildOptions::default(),
        )
    };
    let Some((directory, manifest)) = owning_package(file)? else {
        return Ok(bare());
    };
    let root = match (&manifest.package, &manifest.library) {
        (Some(package), _) => package.root(),
        (None, Some(library)) => library.base_root(),
        // A `[project]` root: no package owns this file.
        (None, None) => return Ok(bare()),
    };
    let options = manifest
        .build_options()
        .map_err(|error| format!("invalid {}/vilan.toml: {error}", directory.display()))?;
    let workspace = vilan_core::manifest::resolve_workspace(&directory, &git_deps())
        .map_err(|error| error.to_string())?;
    Ok((directory.join(root), workspace, options))
}

/// Compiles and executes one test. `Ok` if it exits 0; otherwise `Err(detail)`
/// with the captured runtime output (empty for a compile error, which
/// `compile_to_js` has already reported to stderr).
fn run_test(file: &Path) -> Result<(), String> {
    let (pkg_root, workspace, options) = test_context(file)?;
    let compiled = compile_to_js(
        file,
        &pkg_root,
        Platform::default(),
        CompileGoal::Emit,
        &options,
        &workspace,
        false,
        None,
        None,
    )
    .map_err(|_| String::new())?;
    let script = env::temp_dir().join(format!("vilan-test-{}.mjs", std::process::id()));
    if let Err(error) = fs::write(&script, compiled.javascript) {
        return Err(format!("cannot write {}: {error}", script.display()));
    }
    let output = std::process::Command::new("node").arg(&script).output();
    let _ = fs::remove_file(&script);
    match output {
        Ok(result) if result.status.success() => Ok(()),
        Ok(result) => {
            let mut detail = String::from_utf8_lossy(&result.stdout).into_owned();
            detail.push_str(&String::from_utf8_lossy(&result.stderr));
            Err(detail.trim_end().to_string())
        }
        Err(error) => Err(format!("failed to launch `node`: {error}")),
    }
}

/// The `*_test.vl` files to run: a single file, the test files directly in a given
/// directory, or — with no path — those in the project root (nearest `vilan.toml`).
fn discover_tests(path: Option<PathBuf>) -> Result<Vec<PathBuf>, String> {
    let directory = match path {
        Some(path) if path.is_file() => return Ok(vec![path]),
        Some(path) if path.is_dir() => path,
        Some(path) => return Err(format!("{} does not exist", path.display())),
        None => {
            let working_dir = env::current_dir()
                .map_err(|error| format!("cannot read the working directory: {error}"))?;
            find_project_root(&working_dir).ok_or_else(|| {
                "no `vilan.toml` found here or in any parent directory; \
                 pass a test file or directory"
                    .to_string()
            })?
        }
    };
    let mut tests: Vec<PathBuf> = fs::read_dir(&directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("_test.vl"))
        })
        .collect();
    tests.sort();
    Ok(tests)
}

/// The `std` package directory, resolved in order (proposal/releases.md §3):
/// `$VILAN_STD`; the nearest ancestor of the entry file containing
/// `vilan/std/vilan.toml` — a checkout, so a `vilan` built from this repo
/// compiles against the working tree; else the binary's own embedded std,
/// materialized once to `~/.vilan/std-cache/<hash>/` — what an installed binary
/// uses, from any directory, with no checkout. `resolve_std` reads the resulting
/// package's `[library]` manifest (or, if `$VILAN_STD` points at a bare source
/// root with no manifest, uses it as the base layer).
///
/// # The working directory is not a toolchain (tracker N56)
///
/// The ancestor walk used to run a SECOND time from the process working
/// directory, so a file addressed by absolute path was compiled against
/// whichever checkout the shell happened to be standing in. That is not a
/// fallback, it is a different toolchain: `vilan check ~/code/app/src/x.vl` from
/// inside this repository expanded the application's derives against the working
/// tree's `std`, and where the two versions differ every derive fails at once —
/// 37 `macro PartialEq's definition did not compile` from the vilan tree, 0 from
/// the application's own directory, on one unchanged file. `file_project` already
/// resolves the PACKAGE from the file's own location (G20); the std it compiles
/// against has to come from the same place.
///
/// The one thing the working directory legitimately answers for is a file that
/// belongs to nothing — a bare `.vl` with no `vilan.toml` at or above it, which
/// is a scratch program and not a package's module. `vilan check /tmp/probe.vl`
/// run from a checkout means the checkout's std, because there is no other
/// toolchain in the question; a file INSIDE a package has one, and it is the
/// package's. (An entry that does not exist yet keeps the working directory too:
/// there is no location to ask, and the read error is reported either way.)
fn std_dir(entry: &Path) -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("VILAN_STD") {
        return Ok(PathBuf::from(path));
    }
    let entry_dir = entry
        .canonicalize()
        .ok()
        .and_then(|file| file.parent().map(Path::to_path_buf));
    let in_a_package = entry_dir.as_deref().and_then(find_project_root).is_some();
    let working_dir = if in_a_package {
        None
    } else {
        env::current_dir().ok()
    };
    for start in [entry_dir, working_dir].into_iter().flatten() {
        let mut directory = Some(start.as_path());
        while let Some(current) = directory {
            let candidate = current.join("vilan").join("std");
            if candidate.join("vilan.toml").is_file() {
                return Ok(candidate);
            }
            directory = current.parent();
        }
    }
    vilan_embedded_std::materialize()
}

/// Runs the full pipeline (lex -> parse -> analyze -> contexts -> async infer ->
/// transform) over `file` and reports any diagnostics. Returns the JavaScript on
/// success, or a failure exit code (after reporting) on any error.
/// Writes the build's accumulated assets (const-eval.md §3) beside the
/// compiled output: `<output>.css` for kind "css", deduplicated and
/// deterministically ordered by `assemble_assets`.
///
/// Returns the STYLE SIDECAR's file name when this build emitted one — the
/// `styles` field of the leg's build manifest ([`write_chunks`]). A leg with no
/// `const style()` collects no `css` entry and so writes no file, and that
/// absence is the fact a shell cannot see today (`fullstack-dx.md` §5.2, F1/F2):
/// the manifest states it positively instead of leaving a server to probe the
/// filesystem for it.
///
/// This is also where the previous flush's leftovers go, BOTH mechanisms
/// together (backlog G8): the per-kind prune for every recordable kind, and
/// [`sweep_stale_sidecar`] for `css`. The sidecar sweep used to live only in
/// [`write_chunks`], which `build` calls and `run` / the single-file watch
/// round do not — so a `<entry>.css` survived `vilan run` after the styles
/// that produced it were deleted, while `vilan build` on the same tree removed
/// it. The flush is the one place that knows what this round emitted, so it is
/// where BOTH prunes belong; `write_chunks` keeps its own call for the HMR
/// watch loop, which writes its sidecar directly rather than through here.
fn write_assets(
    output_js: &std::path::Path,
    assets: &[vilan_core::const_eval::EmittedAsset],
) -> Option<String> {
    let directory = output_js.parent().unwrap_or(std::path::Path::new("."));
    let leg = leg_name(output_js);
    let assembled = vilan_core::const_eval::assemble_assets(assets);
    let flushed: BTreeSet<String> = assembled
        .keys()
        .filter(|kind| recordable_emit_kind(kind))
        .cloned()
        .collect();
    prune_and_record_asset_kinds(directory, &leg, &flushed);
    let mut styles = None;
    for (kind, content) in assembled {
        let path = asset_kind_path(directory, &leg, &kind);
        let is_styles = kind.as_str() == "css" && !content.is_empty();
        if let Err(error) = fs::write(&path, content) {
            eprintln!(
                "{} cannot write {}: {error}",
                paint::error_prefix(),
                path.display()
            );
            continue;
        }
        println!(
            "{}  {}",
            paint::out(paint::Style::GREEN, "Emitted"),
            paint::out(paint::Style::BOLD, &path.display().to_string())
        );
        // The write and the record are one call apart, on purpose: the report
        // can never name a file this build did not flush (G11).
        explain::emitted(path.clone(), &leg, &kind);
        if is_styles {
            styles = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned());
        }
    }
    sweep_stale_sidecar(output_js, styles.as_deref());
    styles
}

/// The leg an output path belongs to: the stem `<leg>.<ext>` is named after.
/// A workspace leg carries its manifest name instead — same string, reached
/// without a filesystem round trip — but everything a leg name is USED for is
/// keyed on this one: the kind flush's file, both build records' rows, and the
/// group `--explain` reports under.
fn leg_name(output_path: &std::path::Path) -> String {
    output_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The file a leg's `kind` flush writes: `<leg>.<kind>` beside the bundle.
/// This is THE kind-to-file mapping — the flush writes through it and the
/// per-kind prune removes through it, so the two can never name different
/// files. E94 fences a kind to one path segment, so the result is always one
/// file directly in `directory`.
fn asset_kind_path(directory: &std::path::Path, leg: &str, kind: &str) -> PathBuf {
    directory.join(format!("{leg}.{kind}"))
}

/// Whether the per-kind prune records — and so may later remove — `kind`'s
/// file. Two refusals, from two different places:
///
/// - **The names a leg's build already owns through OTHER machinery**, which
///   is [`vilan_core::const_eval::build_owned_emit_kind`] — `css` belongs to
///   [`sweep_stale_sidecar`], the bundle (`js`/`mjs`), the manifest
///   (`chunks.json`) and the `<arm>.js` route-chunk shapes to
///   [`write_chunks`]'s sweeps, and `vl` is the SOURCE. That list is NOT
///   written here: since G7 the emit-time fence refuses the same list (all of
///   it but `css`, which `emit` is the sanctioned writer of), and two copies
///   of a list are how the write side and the prune side come to disagree
///   about what the build owns. One list, two consumers.
/// - **The record's own line format**: a kind carrying a separator or a line
///   break cannot ride a `leg/kind` line, and that is this consumer's
///   constraint alone.
///
/// Refusal means "never pruned", not "never written". Since G7 the emit fence
/// makes the first half unreachable in practice — a reserved kind never
/// reaches a flush at all — and it stays as the prune's own guard, because
/// what the prune may DELETE should not depend on a check made elsewhere.
fn recordable_emit_kind(kind: &str) -> bool {
    !kind.is_empty()
        && !kind.contains(['/', '\\', '\n', '\r'])
        && vilan_core::const_eval::build_owned_emit_kind(kind).is_none()
}

/// The build's own record of the non-`css` asset kinds each leg's last flush
/// wrote — one `leg/kind` line per file, sorted, beside the outputs (in
/// `dist/` for a workspace, beside the entry for a bare build). It exists
/// exactly while some leg flushes a recordable kind; [`write_leg_record`]
/// removes it with its last entry, so the record never becomes its own stale
/// artifact.
///
/// This is what lets the NEXT build prune a kind's file when the kind stops
/// emitting (backlog G6; `build-hooks.md` §10 Q7's per-kind half) without
/// guessing: the flush is the one place that knows which files it named
/// ([`asset_kind_path`]), so it writes that fact down, and the pruner acts
/// ONLY on what the record says — a file the record does not name is never
/// touched, however kind-shaped its name. The general "delete whatever this
/// build did not write" sweep is deliberately NOT built here (E92 carries
/// it): deleting unrecorded files needs its own ruling.
const ASSET_KIND_RECORD: &str = ".vilan-asset-kinds";

/// The same record for the copies [`write_bundled`] carried (backlog G13), and
/// a SEPARATE file rather than more rows in [`ASSET_KIND_RECORD`]. The reason
/// is one reason wearing three costumes — a record is a row *and* the file that
/// row names, and these two rows name their files by different rules:
///
/// - A kind row names [`asset_kind_path`]'s `<leg>.<kind>`; a bundled row names
///   its target VERBATIM. In one file `client/logo.svg` would be ambiguous
///   between `dist/client.logo.svg` and `dist/logo.svg`, and a pruner that
///   guessed wrong would delete a file it does not own — the one thing G6's law
///   forbids.
/// - A bundled name may carry `/`: a subdirectory is what `bundle` keeps ("the
///   path is the name") and what `bundle_as` may spell, and a subdirectory is
///   the sanctioned escape from a name a leg's build owns. E94 fences a kind to
///   one segment instead. One file would need two row grammars to tell a
///   nested bundle from a leg-and-kind pair.
/// - Each record is removed with its last row, so that it never becomes its own
///   stale artifact. Sharing would tie one mechanism's tidiness to whether the
///   other happened to have something to say.
///
/// What the two DO share is the row shape and both primitives below, so there
/// is one record format and one pruning law, not two.
const BUNDLED_RECORD: &str = ".vilan-bundled";

/// A record's `(leg, name)` rows, keeping only those the reading pruner may
/// act on. A missing or unreadable record reads as empty — nothing is pruned,
/// because pruning without a record would mean guessing at filenames. A row
/// that does not parse, or names something the prune may not touch, is dropped
/// on the same principle.
///
/// The split is at the FIRST `/`, which is unambiguous whatever the right half
/// holds: a leg name is a manifest-checked identifier or a file stem, so it can
/// never contain one.
fn read_leg_record(path: &std::path::Path, recordable: fn(&str) -> bool) -> Vec<(String, String)> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let (leg, name) = line.split_once('/')?;
            (!leg.is_empty() && recordable(name)).then(|| (leg.to_string(), name.to_string()))
        })
        .collect()
}

/// Writes a record back — sorted, so its bytes are a function of the set of
/// rows — or removes it when nothing is left to record. A failure is reported
/// and otherwise ignored, like the sweeps': a record is bookkeeping for the
/// NEXT build's tidiness, never this build's correctness. `noun` names the
/// record in that report, because a reader deserves to know which of the two
/// could not be written.
fn write_leg_record(path: &std::path::Path, mut entries: Vec<(String, String)>, noun: &str) {
    entries.sort();
    entries.dedup();
    if entries.is_empty() {
        if path.is_file()
            && let Err(error) = fs::remove_file(path)
        {
            eprintln!(
                "{} cannot remove the {noun} {}: {error}",
                paint::warning_prefix(),
                path.display()
            );
        }
        return;
    }
    let mut text = entries
        .iter()
        .map(|(leg, name)| format!("{leg}/{name}"))
        .collect::<Vec<_>>()
        .join("\n");
    text.push('\n');
    if let Err(error) = fs::write(path, text) {
        eprintln!(
            "{} cannot write the {noun} {}: {error}",
            paint::warning_prefix(),
            path.display()
        );
    }
}

/// Removes the kind files `leg`'s previous flush wrote and this one did not —
/// the per-kind prune (backlog G6): a kind that stops emitting stops
/// shipping, because a stale flush in `dist/` SHIPS, which is worse than a
/// missing file under "a built app needs nothing but `dist/`". Then
/// re-records what this flush did write, so the next build can do the same.
///
/// Only `leg`'s own rows move. A skipped watch round never reaches here (its
/// files are still current), and another leg's rows — including a leg no
/// longer in the workspace, whose leftovers are E92's general-sweep
/// territory — carry through untouched. A failed removal is reported and
/// otherwise ignored, exactly as the chunk sweep treats a stray.
fn prune_and_record_asset_kinds(
    directory: &std::path::Path,
    leg: &str,
    flushed: &BTreeSet<String>,
) {
    if leg.is_empty() {
        return;
    }
    let record = directory.join(ASSET_KIND_RECORD);
    let mut entries = read_leg_record(&record, recordable_emit_kind);
    for (recorded_leg, kind) in &entries {
        if recorded_leg != leg || flushed.contains(kind) {
            continue;
        }
        let path = asset_kind_path(directory, leg, kind);
        if !path.is_file() {
            continue;
        }
        if let Err(error) = fs::remove_file(&path) {
            eprintln!(
                "{} cannot remove the stale asset {}: {error}",
                paint::warning_prefix(),
                path.display()
            );
        }
    }
    entries.retain(|(recorded_leg, _)| recorded_leg != leg);
    for kind in flushed {
        entries.push((leg.to_string(), kind.clone()));
    }
    write_leg_record(&record, entries, "asset-kind record");
}

/// Whether the bundle prune records — and so may later remove — the copy that
/// landed on `name`. This is the prune's OWN guard, kept for the reason
/// [`recordable_emit_kind`] keeps one: what a build may DELETE must not depend
/// on a check made somewhere else. Every shape refused here is one
/// `const_eval`'s `bundled_name` / `bundled_target` fences already refuse
/// before a copy is registered — an absolute target, a `..`, a `.` — so the
/// guard is not expected to fire; the one refusal it owns alone is a name
/// carrying a line break, which cannot ride a `leg/name` line.
///
/// Refusal means "never pruned", not "never copied": a copy this cannot record
/// is simply one the next build leaves where it is.
fn recordable_bundled_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['\n', '\r'])
        && std::path::Path::new(name)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

/// Removes the copies `leg`'s previous build carried and this one no longer
/// names, then re-records what it did carry — the bundled half of the doctrine
/// [`prune_and_record_asset_kinds`] already enforces for kinds, and the same
/// law rather than a second one (backlog G13).
///
/// A stale file in `dist/` SHIPS, which is worse than a missing one: `dist/` is
/// the deploy artifact, so a static host in front of it serves a `robots.txt`
/// the build stopped naming forever. `serve_build` routes from the manifest and
/// would not, which is exactly why the gap was invisible. Two reachable ways in
/// with no source edit at all: `read_dir_all` makes a DELETION from a listed
/// tree invisible to the copy, and a `digest`-minted fingerprint orphans one
/// copy per save under `--watch`, unbounded for the life of the session.
///
/// The pruner acts ONLY on its own record. A user-placed file, another leg's
/// outputs, and anything unrecorded are untouchable however bundle-shaped their
/// names — G6's law, and it stays one law.
///
/// Rows are keyed by leg because `dist/` is one directory that every leg copies
/// into. But a name is NOT one leg's to delete merely because that leg stopped
/// naming it: two legs bundling the same file to the same target is legal and
/// expected (the copy is idempotent, and `write_bundled` refuses only two
/// DIFFERENT sources on one name). So the prune removes a file only when the
/// record it is about to WRITE no longer names it at all — this leg's rows
/// replaced, every other leg's carried through.
fn prune_and_record_bundled(directory: &std::path::Path, leg: &str, bundled: &BTreeSet<String>) {
    if leg.is_empty() {
        return;
    }
    let record = directory.join(BUNDLED_RECORD);
    let previous = read_leg_record(&record, recordable_bundled_name);
    let mut next: Vec<(String, String)> = previous
        .iter()
        .filter(|(recorded_leg, _)| recorded_leg != leg)
        .cloned()
        .collect();
    for name in bundled {
        if recordable_bundled_name(name) {
            next.push((leg.to_string(), name.clone()));
        }
    }
    let kept: BTreeSet<&str> = next.iter().map(|(_, name)| name.as_str()).collect();
    for (recorded_leg, name) in &previous {
        if recorded_leg != leg || kept.contains(name.as_str()) {
            continue;
        }
        let path = directory.join(name);
        if !path.is_file() {
            continue;
        }
        if let Err(error) = fs::remove_file(&path) {
            eprintln!(
                "{} cannot remove the stale bundled asset {}: {error}",
                paint::warning_prefix(),
                path.display()
            );
        }
    }
    write_leg_record(&record, next, "bundled-asset record");
}

/// Copies every file `const asset::bundle` registered into the build's output
/// directory, and returns their names — the `assets` array of the leg's build
/// manifest ([`write_chunks`]), in the order the program asked for them.
///
/// This is the half of kolt.local 029 that makes `dist/` self-sufficient: the
/// const channel decides WHICH files ride the build (and so which do not — a
/// resource no `const` names is never copied, which is the reachability the
/// item asked the compiler to keep), and this decides where they land.
///
/// Three refusals, each a real collision the copy would otherwise hide:
///
/// - **A source that is already its own destination** is left alone, not
///   copied. A bare file bundling a sibling (`vilan build app.vl` with
///   `const bundle("note.txt")` beside it) resolves source and destination to
///   one path, and `fs::copy` over itself TRUNCATES — the file the build was
///   asked to carry would be destroyed by carrying it.
/// - **A name a leg's own build owns** ([`LegNamespace`]) is refused and fails
///   the build. Copying over `client.js` is the same lie a stale chunk is and
///   it is silent; worse, `sweep_stale_chunks` and `sweep_stale_sidecar` would
///   DELETE a bundled `client.arm.js` or `client.css` on the next build, so a
///   resource parked on one of those names does not merely collide — it
///   disappears.
/// - **A name two legs both bundle** is fine and expected — it is the same
///   package-relative file, so the copy is idempotent. A name two legs bundle
///   from DIFFERENT files is not, and is refused: `asset::bundle_as` lets a
///   target be spelled at the call (const-eval.md §3.1), so the identity rule
///   that used to make this impossible — the path IS the name — no longer
///   does, and one `dist/` cannot serve two files on one url. The const pass
///   catches the collision within a compile; this catches it ACROSS legs,
///   which are separate compiles into one directory, and `written` is what
///   carries the earlier leg's answer here.
///
/// A failed copy fails the build. Unlike a stray chunk, a missing asset is not
/// a tidiness problem: the manifest is about to name it, and `load_build`
/// stops a server whose build names a file that is not on disk.
///
/// Every copy that MOVED BYTES is recorded, so the next build can prune the one
/// this build stopped naming ([`prune_and_record_bundled`]) — the sweep the
/// three beside this one already perform for their own outputs.
fn write_bundled(
    output_directory: &std::path::Path,
    bundled: &[(PathBuf, String)],
    // Whose copies these are: the leg name `--explain` groups them under, so a
    // report can tell `client`'s copy of a shared file from `server`'s, AND the
    // key the bundled record files them under, so one leg's prune cannot reach
    // another's copies. Empty means neither — a caller with no leg to name
    // records nothing and prunes nothing.
    leg: &str,
    reserved: &[LegNamespace],
    written_names: &mut BTreeMap<String, PathBuf>,
) -> Result<Vec<String>, ExitCode> {
    let mut written = Vec::new();
    // What the record will say. Not the same list as `written`: a resource that
    // is ALREADY its own destination is carried without being copied, and
    // recording it would put the build's own SOURCE tree on a future build's
    // deletion list — the file `same_file` exists to protect would be destroyed
    // by the sweep instead of by `fs::copy`.
    let mut recorded = BTreeSet::new();
    for (source, name) in bundled {
        match written_names.get(name) {
            Some(earlier) if earlier != source => {
                eprintln!(
                    "{} `{}` and `{}` both bundle to `{name}` — one build directory \
                     cannot serve two files on one url; give one of them a target of \
                     its own with `asset::bundle_as`",
                    paint::error_prefix(),
                    earlier.display(),
                    source.display()
                );
                return Err(ExitCode::FAILURE);
            }
            _ => {
                written_names.insert(name.clone(), source.clone());
            }
        }
        if let Some((claimant, role)) = reserved
            .iter()
            .find_map(|namespace| namespace.claims(name).map(|role| (&namespace.leg, role)))
        {
            eprintln!(
                "{} `{name}` is the `{claimant}` leg's {role}, so `{}` cannot be bundled \
                 there — move the resource into a subdirectory, or give it a target \
                 of its own with `asset::bundle_as`",
                paint::error_prefix(),
                source.display()
            );
            return Err(ExitCode::FAILURE);
        }
        let destination = output_directory.join(name);
        // Same file, so the copy is already done — and doing it would undo it.
        if same_file(source, &destination) {
            // Recorded anyway: it is a resource this build carries, and a
            // report that named only the copies that moved bytes would leave a
            // reader hunting for the one file it skipped.
            explain::bundled(destination, leg, source.clone(), name);
            written.push(name.clone());
            continue;
        }
        if let Some(parent) = destination.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            eprintln!(
                "{} cannot create {}: {error}",
                paint::error_prefix(),
                parent.display()
            );
            return Err(ExitCode::FAILURE);
        }
        if let Err(error) = fs::copy(source, &destination) {
            eprintln!(
                "{} cannot bundle {} into {}: {error}",
                paint::error_prefix(),
                source.display(),
                destination.display()
            );
            return Err(ExitCode::FAILURE);
        }
        println!(
            "{}  {}",
            paint::out(paint::Style::GREEN, "Bundled"),
            paint::out(paint::Style::BOLD, &destination.display().to_string())
        );
        explain::bundled(destination, leg, source.clone(), name);
        written.push(name.clone());
        recorded.insert(name.clone());
    }
    // After the copies, on the success path alone: a build that failed halfway
    // has not written the tree its record would be describing, and a DELETION
    // on the way to a failure is the worst outcome available.
    prune_and_record_bundled(output_directory, leg, &recorded);
    Ok(written)
}

/// The names in the output directory one leg's build owns. A bundled resource
/// may not take any of them, and the reason is not only collision: the two
/// sweeps in [`write_chunks`] treat everything in this namespace as theirs to
/// DELETE when a build stops producing it, so a resource parked here would be
/// removed by the next build rather than merely overwritten by this one.
struct LegNamespace {
    leg: String,
    /// The bundle's extension — `js` on a browser leg, `mjs` on a process leg.
    extension: &'static str,
}

impl LegNamespace {
    fn of(leg: &str, platform: Platform) -> LegNamespace {
        LegNamespace {
            leg: leg.to_string(),
            extension: platform.script_extension(),
        }
    }

    /// Why this leg's build claims `name`, or `None` if the name is free. The
    /// role is a noun phrase for the refusal message.
    fn claims(&self, name: &str) -> Option<&'static str> {
        let leg = &self.leg;
        if name == format!("{leg}.{}", self.extension) {
            return Some("compiled bundle");
        }
        // Reserved whether or not this build emitted one: `sweep_stale_sidecar`
        // removes `<leg>.css` exactly when the leg emitted no styles, so a
        // resource on that name is deleted by the build that did not write it.
        if name == format!("{leg}.css") {
            return Some("style sidecar");
        }
        if name == format!("{leg}.chunks.json") {
            return Some("build manifest");
        }
        // `<leg>.<arm>.js`, the whole route-chunk namespace — a pattern rather
        // than a name, because `sweep_stale_chunks` removes by that pattern.
        if name
            .strip_prefix(&format!("{leg}."))
            .and_then(|rest| rest.strip_suffix(".js"))
            .is_some_and(|arm| !arm.is_empty())
        {
            return Some("route-chunk namespace");
        }
        None
    }
}

/// Whether two paths name one file on disk. Canonicalized rather than compared
/// textually: `vilan build ./app.vl` and the package root resolve the same file
/// through different spellings, and a symlinked resource is still the file it
/// points at. A path that cannot be canonicalized (the destination usually does
/// not exist yet) is not the same file as anything.
///
/// Resolving BOTH sides means a `dist/` entry that is a link back at the source
/// answers "already copied", and the build carries it without rewriting bytes.
/// Recorded (audit run 6's F24) and **settled** rather than fenced: under G19's
/// ruling a symlink is a supported spelling of project layout, in the output
/// directory as anywhere else, and the alternative — comparing the destination
/// unresolved — would make `vilan build` overwrite a link somebody put there on
/// purpose, which is the outcome this predicate exists to prevent for the source
/// itself. The copy is a build product either way, and the sweep that removes a
/// resource no leg names any more removes it by the name it recorded.
fn same_file(left: &std::path::Path, right: &std::path::Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// Writes a browser leg's BUILD MANIFEST — `<leg>.chunks.json` — plus the route
/// chunks it describes, when the leg split.
///
/// The sidecar started as a list of chunk files a hand-written server could
/// iterate instead of hard-coding one route per file (`bundle-splitting.md` §3).
/// `fullstack-dx.md` §10.3 (RATIFIED 2026-08-11) made it the leg's build
/// manifest — *what this leg's build emitted*, the value `std::build::build_of`
/// reads and `serve_build` serves — which means it is written on EVERY build of
/// a browser leg, chunks or none, carrying:
///
/// - `leg` / `entry` — the leg's name and its eager bundle's file name;
/// - `styles` — the style sidecar's file name, or `null` when the leg compiled
///   no styles (F1/F2: the fact a shell cannot see and a `fs::stat` probe
///   cannot check in both directions);
/// - `classic_script` — whether the bundle must be loaded as a classic script,
///   true exactly when the leg split, because chunk resolution reads
///   `document.currentScript` (§3.5);
/// - `chunks` — the route chunks, `[]` for a leg that does not split;
/// - `assets` — the files `const asset::bundle` carried into `dist/`, `[]` for
///   a leg that bundled none (kolt.local 029).
///
/// That reverses `bundle-splitting.md` §9's "dropping `split` takes the manifest
/// with it", and STRENGTHENS its invariant rather than weakening it: the
/// invariant is *the leg's last build owns the file*, and a present-but-empty
/// chunk list is a positive statement where an absent file is an ambiguity
/// between "did not split" and "was never built" (§5.9). `build_of` needs that
/// difference — a leg that was never built is a named error, not an empty build.
///
/// A NODE leg writes none: the manifest describes what a browser loads, and
/// `classic_script` has no meaning off the browser.
///
/// Chunk names are `<leg>.<arm>.js`, and a leg name is a manifest-checked
/// identifier (no `.`), so a chunk can never collide with another leg's
/// `dist/<leg>` — which is why `reject_output_collisions` needs no chunk
/// pass of its own. That same shape is what makes the sweep below safe: every
/// `<leg>.<anything>.js` beside the bundle is this leg's chunk and nobody
/// else's.
///
/// EVERY write of a leg goes through here, `chunks` empty or not, because the
/// leg's chunk namespace belongs to its LAST build (`bundle-splitting.md` §S3,
/// item 4): renaming a route arm, dropping `split`, or a `--watch` round — which
/// emits the whole bundle by design — must not leave the previous build's chunk
/// files lying beside it. They would be inert (a whole bundle names no chunk)
/// but a `chunks.json` that outlived its chunks is a manifest that lies, and a
/// server iterating it would serve code the bundle no longer knows about.
fn write_chunks(
    output_js: &std::path::Path,
    chunks: &[EmittedChunk],
    styles: Option<&str>,
    // The names `write_bundled` just copied for this leg, in the program's own
    // order — the manifest's `assets` array, and so what `serve_build` serves
    // without a route of the app's own (kolt.local 029).
    assets: &[String],
    is_browser: bool,
) -> Result<(), ExitCode> {
    let directory = output_js.parent().unwrap_or(std::path::Path::new("."));
    // The manifest this build is about to write is not stale; one left by a
    // build that wrote a manifest where this one will not (a leg retargeted off
    // the browser) is, and the sweep takes it.
    sweep_stale_chunks(output_js, chunks, is_browser);
    // For the HMR watch loop, which writes its sidecar straight into `dist/`
    // rather than through [`write_assets`] — every other caller has already
    // swept there, and a second call on the same `styles` is a no-op (G8).
    sweep_stale_sidecar(output_js, styles);
    for chunk in chunks {
        let path = directory.join(&chunk.file);
        if let Err(error) = fs::write(&path, &chunk.source) {
            eprintln!(
                "{} cannot write {}: {error}",
                paint::error_prefix(),
                path.display()
            );
            return Err(ExitCode::FAILURE);
        }
        println!(
            "{}    {} ({})",
            paint::out(paint::Style::GREEN, "Chunk"),
            paint::out(paint::Style::BOLD, &path.display().to_string()),
            chunk.arm
        );
        explain::chunk(
            path,
            &output_js
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default(),
            &chunk.arm,
        );
    }
    if !is_browser {
        return Ok(());
    }
    let leg = output_js
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let entry = output_js
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let listing = if chunks.is_empty() {
        "[]".to_string()
    } else {
        let rows = chunks
            .iter()
            .map(|chunk| {
                format!(
                    "\t\t{{ \"arm\": {}, \"tag\": {}, \"file\": {} }}",
                    json_string(&chunk.arm),
                    chunk.tag,
                    json_string(&chunk.file)
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");
        format!("[\n{rows}\n\t]")
    };
    let bundled = if assets.is_empty() {
        "[]".to_string()
    } else {
        let rows = assets
            .iter()
            .map(|name| format!("\t\t{}", json_string(name)))
            .collect::<Vec<_>>()
            .join(",\n");
        format!("[\n{rows}\n\t]")
    };
    let manifest = format!(
        "{{\n\t\"leg\": {},\n\t\"entry\": {},\n\t\"styles\": {},\n\t\"classic_script\": {},\n\t\"chunks\": {listing},\n\t\"assets\": {bundled}\n}}\n",
        json_string(&leg),
        json_string(&entry),
        styles.map_or_else(|| "null".to_string(), json_string),
        // A split leg's chunk `import()` resolves against
        // `document.currentScript`, which is `null` in a module script — so a
        // leg that split must be loaded by a classic `<script>` and a leg that
        // did not may be loaded either way (`fullstack-dx.md` §3.5, F6).
        !chunks.is_empty(),
    );
    let manifest_path = output_js.with_extension("chunks.json");
    if let Err(error) = fs::write(&manifest_path, manifest) {
        eprintln!(
            "{} cannot write {}: {error}",
            paint::error_prefix(),
            manifest_path.display()
        );
        return Err(ExitCode::FAILURE);
    }
    // Explained ALWAYS, and announced only when it describes chunks — the two
    // are different questions. The build log is a running commentary and says
    // nothing routine; `--explain` is an inventory of `dist/`, and a file it
    // left out would be the one the reader came to look up.
    explain::manifest(manifest_path.clone(), &leg);
    // Announced only when it describes chunks: a non-splitting leg's manifest is
    // as routine as its `.css` sidecar and says nothing a reader of a build log
    // needs. (Byte-identical `vilan build` output for every project that has one
    // today, which is every project.)
    if !chunks.is_empty() {
        println!(
            "{}    {}",
            paint::out(paint::Style::GREEN, "Chunk"),
            paint::out(paint::Style::BOLD, &manifest_path.display().to_string())
        );
    }
    Ok(())
}

/// Removes the chunk artifacts of `output_js`'s leg that this build did not
/// write. `<leg>.<arm>.js` and `<leg>.chunks.json` are the leg's own namespace
/// (a leg name is an identifier, so it holds no `.`), and the last build of the
/// leg owns all of it. A failed removal is reported and otherwise ignored: a
/// stray is a tidiness problem, never a correctness one, and it must not fail a
/// build that otherwise succeeded.
///
/// `writes_manifest` says whether the caller is about to write `<leg>.chunks.json`
/// itself — true for every browser leg since `fullstack-dx.md` §10.3 made the
/// sidecar the leg's build manifest. The manifest is swept only when this build
/// will leave none, which is a leg retargeted off the browser: a manifest
/// describing a bundle no browser loads is the same lie as one outliving its
/// chunks.
fn sweep_stale_chunks(output_js: &std::path::Path, wrote: &[EmittedChunk], writes_manifest: bool) {
    let directory = output_js.parent().unwrap_or(std::path::Path::new("."));
    let Some(leg) = output_js.file_stem().map(|stem| stem.to_string_lossy()) else {
        return;
    };
    let manifest = format!("{leg}.chunks.json");
    let keep: Vec<&str> = wrote.iter().map(|chunk| chunk.file.as_str()).collect();
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // `<leg>.<arm>.js`, with a non-empty arm — which `<leg>.js` itself, the
        // bundle this is called to protect, does not match.
        let is_chunk = name
            .strip_prefix(&format!("{leg}."))
            .and_then(|rest| rest.strip_suffix(".js"))
            .is_some_and(|arm| !arm.is_empty());
        let is_manifest = name == manifest;
        if (!is_chunk && !is_manifest) || keep.contains(&name.as_str()) {
            continue;
        }
        if is_manifest && writes_manifest {
            continue;
        }
        if let Err(error) = fs::remove_file(entry.path()) {
            eprintln!(
                "{} cannot remove the stale chunk {}: {error}",
                paint::warning_prefix(),
                entry.path().display()
            );
        }
    }
}

/// Removes `<leg>.css` when this build wrote none — the style sidecar's half of
/// the doctrine [`sweep_stale_chunks`] already enforces for chunks: the leg's
/// dist namespace belongs to its LAST build (`bundle-splitting.md` §S3, item 4).
/// `styles` is what [`write_assets`] just wrote for this leg, so `None` means
/// the source stopped emitting `css` — and a sidecar that outlives the source
/// that emitted it is exactly the lie a `chunks.json` outliving its chunks is:
/// the manifest beside it says `"styles": null` while the file sits there, a
/// server probing the filesystem finds a stylesheet the build did not produce,
/// and under `--watch` the dev channel keeps serving it (kolt.local 007 — the
/// browser then RE-INJECTS the deleted stylesheet, which is resurrection, not
/// staleness). A failed removal is reported and otherwise ignored, exactly as
/// the chunk sweep treats a stray.
///
/// Called from [`write_assets`] — every path that flushes assets, `build` and
/// `run` and both watch loops alike (backlog G8; before that it hung off
/// [`write_chunks`], which only `build` reaches) — and once more from
/// [`write_chunks`] for the HMR loop, which never flushes through
/// `write_assets`. It touches ONE name, `<leg>.css`, so a user file beside the
/// entry is not in its reach.
fn sweep_stale_sidecar(output_js: &std::path::Path, styles: Option<&str>) {
    if styles.is_some() {
        return;
    }
    let sidecar = output_js.with_extension("css");
    if !sidecar.is_file() {
        return;
    }
    if let Err(error) = fs::remove_file(&sidecar) {
        eprintln!(
            "{} cannot remove the stale stylesheet {}: {error}",
            paint::warning_prefix(),
            sidecar.display()
        );
    }
}

/// A JSON string literal. Chunk arms and file names come from vilan identifiers
/// and the emitter's own sanitizer, so this only has to be correct, not clever.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            character if (character as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", character as u32))
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

/// Set once by `build --print-chunks` before any compile, read at the one
/// place the analyzed `Program` is live (`compile_to_js`). A process-level
/// flag rather than a parameter because `compile_unit` is shared by `run`,
/// `check`, and the watch loops — threading a report-only bool through every
/// signature and call site would touch far more than it informs. Under
/// `--watch` the flag stays set, so every rebuild re-reports — intended.
static PRINT_CHUNKS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn compile_to_js(
    file: &Path,
    pkg_root: &Path,
    platform: Platform,
    goal: CompileGoal,
    options: &BuildOptions,
    workspace: &Workspace,
    emit_debug: bool,
    // When `Some`, an ANSI-free plain-text rendering of this file's diagnostics
    // is written here on a failed compile — the HMR error overlay's copy (hmr.md
    // §§2/§6). The terminal rendering below is untouched: this is a second,
    // additive pass over the SAME messages, never a redirect. Every other caller
    // passes `None` and pays nothing.
    overlay: Option<&mut String>,
    // `Some((leg, sink))` emits route chunks for a `split = true` browser leg:
    // the returned JavaScript is then the EAGER bundle and `sink` receives one
    // entry per chunk file (`bundle-splitting.md` §3).
    split: Option<(&str, &mut Vec<EmittedChunk>)>,
) -> Result<Compiled, ExitCode> {
    // `read_source` drops a leading BOM so spans — and the ariadne rendering
    // below, which indexes this same text — address the source proper
    // (windows-support.md §2).
    let src = match vilan_core::util::read_source(file) {
        Ok(src) => src,
        Err(error) => {
            eprintln!(
                "{} cannot read {}: {error}",
                paint::error_prefix(),
                file.display()
            );
            return Err(ExitCode::FAILURE);
        }
    };
    // Exact case for the entry, checked here because this is the ONE seam every
    // entry compile passes through (`build`, `run`, `check`, `test`, each watch
    // round) — whether the path came from the command line, `[package] entry`,
    // or an `[entry.<name>] path`. It has no source span (nothing in the program
    // names it), so it reports like the read failure above rather than through
    // the diagnostic channel.
    if let Some((requested, on_disk)) = entry_case_mismatch(file, pkg_root) {
        eprintln!(
            "{} entry {} resolved to `{on_disk}` on disk, but it is named `{requested}`: \
             Vilan matches source files by exact case, so this builds only where the \
             filesystem ignores case; rename one to match the other",
            paint::error_prefix(),
            file.display()
        );
        return Err(ExitCode::FAILURE);
    }
    let filename = file.to_string_lossy().into_owned();
    let std_directory = match std_dir(file) {
        Ok(directory) => directory,
        Err(error) => {
            eprintln!("{} {error}", paint::error_prefix());
            return Err(ExitCode::FAILURE);
        }
    };
    let std = vilan_core::manifest::resolve_std(&std_directory);
    let mut output = None;

    // Fast path: a clean entry file reuses the shared content-addressed parse
    // cache (`vilan_core::parse_clean_cached`) — the same cache `std` and the
    // package modules use — so across `--watch` rounds an unchanged entry file
    // is served from the cache instead of re-parsed (backlog E12). A hit is
    // already lift-rewritten and `'static`; the handwritten frontend runs only
    // when the cache misses (a non-clean file), recovering a tree and naming its
    // diagnostics in a single fast-and-rich pass.
    // The depth instrument (B138) anchors before the FIRST recursion of the
    // pipeline, which on this path is the entry's own parse — here, or in the
    // clean-parse cache below — rather than anything inside `analyze` (B139
    // added the parser to the instrument, and it is the family with no bound).
    vilan_core::begin_depth_stats();
    let cached = vilan_core::parse_clean_cached(&src);

    // Analyzer and codegen diagnostics, collected as `(source, span, message)`
    // for ariadne — the source being the file the span indexes into, so each one
    // renders in its own file (backlog E16). Note-carrying ones render
    // separately (they still count against a clean build via `noted_errors`).
    let mut analyzer_errors: Vec<(SourceId, std::ops::Range<usize>, String)> = Vec::new();
    let mut noted_errors = 0usize;
    // Diagnostics this round already rendered for an earlier entry (B182). They
    // are not shown again and they still count: the leg is broken, and only the
    // repetition was dropped.
    let mut repeated_errors = 0usize;
    // The same diagnostics, captured as structured items for the HMR overlay
    // (only assembled into text when `overlay` is `Some`). Populated alongside
    // the terminal path — never in place of it — reusing each message verbatim.
    let mut overlay_diagnostics: Vec<hmr::OverlayDiagnostic> = Vec::new();

    // On a cache miss the handwritten frontend parses the entry, always returning
    // a (possibly recovered) tree alongside every diagnostic.
    //
    // `build` does not analyze a file that failed to parse cleanly — its parse
    // errors are reported and the build fails — so the freshly parsed tree is
    // taken only when the parse produced no diagnostics. `check` DOES analyze it
    // (`editing-dx.md` S6/§13.1): its whole job is to answer questions about a
    // file the user is still writing, and dropping the tree meant one missing `;`
    // anywhere blinded it to everything else in the file (§2.2 mechanism 1,
    // measured as P29). The tree is the same one `analyze_source` has handed the
    // language server since the H6 cutover, now that S1's synchronizer makes it
    // cover the whole file rather than the prefix before the first syntax error.
    let mut parse_errors: Vec<vilan_core::parsing::ParseError> = Vec::new();
    let fresh_root: Option<vilan_core::Spanned<vilan_core::node::NodeList>> = match &cached {
        None => {
            let (tree, errors) = vilan_core::parsing::parse(src.as_str());
            let analyzable = errors.is_empty() || goal.analyzes_recovered_trees();
            parse_errors = errors;
            tree.filter(|_| analyzable).map(|(mut items, span)| {
                // Elements desugar, then bare-`?` marks become lift regions,
                // before analysis (element-syntax.md §4, expression-lifting.md);
                // the cached path gets both inside `parse_clean_cached`, so
                // each runs exactly once here.
                vilan_core::css::rewrite_items(&mut items, src.as_str());
                vilan_core::elements::rewrite_items(&mut items, src.as_str());
                vilan_core::lift::rewrite_items(&mut items);
                (items, span)
            })
        }
        Some(_) => None,
    };
    let root: Option<&vilan_core::Spanned<vilan_core::node::NodeList>> = match &cached {
        Some((ast, _)) => Some(*ast),
        None => fresh_root.as_ref(),
    };
    // The source text the chosen root's spans index into: the cached `'static`
    // text on a hit (byte-identical to `src` — the cache is content-keyed),
    // otherwise `src` itself. The ENTRY's diagnostics render against it.
    let source_ref: &str = match &cached {
        Some((_, text)) => text,
        None => src.as_str(),
    };
    // The text each diagnostic renders against — the file its span indexes
    // into, not the entry's (backlog E16). The entry is seeded from the text
    // just chosen; every other source is read back through the same
    // `read_source` the module loader used, so the offsets line up. One map,
    // shared by the plain path, the note path and the warnings.
    let mut diagnostic_files: HashMap<SourceId, (String, String)> = HashMap::new();
    diagnostic_files.insert(SourceId(0), (filename.clone(), source_ref.to_string()));

    if let Some(root) = root {
        if emit_debug {
            // Two dumps, and they BRACKET the desugars hooked at every parse
            // entry — `css`, `elements`, `lift` (backlog E99). `parse-raw.out`
            // is the tree the frontend produced; `parse.out` is the tree
            // analysis receives, which is the rewritten one and always was, so
            // a node in one and not the other is something a desugar added or
            // removed — the split needed to tell a parser bug from a desugar's.
            //
            // The raw tree comes from a fresh parse of the entry's own text
            // rather than from the branch above, so a clean-parse cache HIT
            // dumps one too: the cache is content-keyed, so this parse
            // reproduces exactly the tree the cached entry was built from. It
            // is one extra parse, paid only under `-d`.
            let (raw_root, _) = vilan_core::parsing::parse(source_ref);
            if let Some(raw_root) = raw_root {
                write_debug(file, "parse-raw.out", &format!("{raw_root:#?}"));
            }
            write_debug(file, "parse.out", &format!("{root:#?}"));
        }

        let mut program = analyze(root, source_ref, &std, pkg_root, file, platform, workspace);

        // The whole-program passes that follow analysis — context threading,
        // async inference, the drop checks, platform coloring, const
        // evaluation, the initializer-cycle check — and the ONE call graph
        // they share. Defined once in `vilan_core` and called by both
        // pipelines: this sequence was written out twice, and a pass added to
        // only one of them is a check the other silently skips.
        vilan_core::post_analysis_passes(
            &mut program,
            platform,
            &vilan_core::options::BuildOptions::default(),
        );

        // Every file `const asset::read` touched is a build input: hand the
        // set to the watcher so a change to one — or the appearance of one
        // that was missing — triggers a round exactly as a `.vl` edit does.
        record_const_inputs(&program.const_input_files);

        // The `const` INFERENCE sweep (const-eval.md §9) — THE ONE CALL SITE.
        // It lives here, on the CLI's build path, and not beside the explicit
        // pass inside `analyze_source`, because the language server and the
        // wasm playground enter through that function and must never run it
        // (§4's tooling split, §9.6): inference is silent-fallback
        // optimization, so it produces nothing an editor could surface.
        // `crates/vilan-core/tests/const_eval_reach.rs` pins the separation at
        // the source level, the way the playground's split guard does.
        //
        // Gated on the `[build]` preset — off under `debug`, on under
        // `release` (§9.4). Runs AFTER `check_cycles` so a program with any
        // diagnostic is already excluded: folds are an optimization over code
        // that compiles, never a way of making code compile.
        program
            .const_results
            .extend(vilan_core::const_eval::infer(&program, options));

        for (index, error) in program.diagnostics.iter().enumerate() {
            let source = program.diagnostic_source(index);
            load_diagnostic_file(&mut diagnostic_files, &program, source);
            // Capture every diagnostic for the overlay (message + note verbatim),
            // located in the file its span indexes into (E16) — a module error
            // names the module, not the leg's entry; the terminal rendering
            // below is unchanged. Its E78 requirement trace rides along (E80),
            // each hop located in ITS file: a hop's `Note::source` names the
            // importing module the call sits in, `None` the anchor's own file.
            for hop in &error.trace {
                load_diagnostic_file(
                    &mut diagnostic_files,
                    &program,
                    hop.note.source.unwrap_or(source),
                );
            }
            let (overlay_name, overlay_text) = diagnostic_file(&diagnostic_files, source);
            let overlay_trace = error
                .trace
                .iter()
                .map(|hop| {
                    let (hop_name, hop_text) =
                        diagnostic_file(&diagnostic_files, hop.note.source.unwrap_or(source));
                    hmr::OverlayTraceEntry::located(
                        hop_name,
                        hop_text,
                        hop.note.span.into_range(),
                        hop.note.msg.clone(),
                        hop.call,
                    )
                })
                .collect();
            overlay_diagnostics.push(
                hmr::OverlayDiagnostic::located(
                    overlay_name,
                    overlay_text,
                    error.span.into_range(),
                    error.msg.clone(),
                    error.note.as_ref().map(|note| note.msg.clone()),
                )
                .with_trace(overlay_trace),
            );
            // B182: a module every leg of a package reaches is analyzed once
            // per leg, so its errors arrive once per leg. Render the first,
            // COUNT the rest — the leg still failed, and dropping it from the
            // verdict would let a second entry emit over a broken module.
            if !first_report_this_round(overlay_name, &error.span.into_range(), &error.msg) {
                repeated_errors += 1;
                continue;
            }
            // A diagnostic carrying secondary locations — an E78 requirement
            // trace and/or a C3 note — renders directly (multi-label; the
            // shared ariadne path has nowhere to put them); plain ones keep
            // the shared path.
            if error.note.is_some() || !error.trace.is_empty() {
                // A cross-source label reads its file so the sub-label can
                // render in it (the trait's declaration in std, a chain hop
                // in an importing module). `None`, or the primary's own
                // source, means the same file — which needs no second source.
                let secondaries = || {
                    error
                        .trace
                        .iter()
                        .map(|hop| &hop.note)
                        .chain(error.note.as_ref())
                };
                for secondary in secondaries() {
                    if let Some(label_source) = secondary
                        .source
                        .filter(|label_source| *label_source != source)
                    {
                        load_diagnostic_file(&mut diagnostic_files, &program, label_source);
                    }
                }
                let (name, text) = diagnostic_file(&diagnostic_files, source);
                let located: Vec<(&vilan_core::error::Note, Option<(&str, &str)>)> = secondaries()
                    .map(|secondary| {
                        let file = secondary
                            .source
                            .filter(|label_source| *label_source != source)
                            .map(|label_source| diagnostic_file(&diagnostic_files, label_source));
                        (secondary, file)
                    })
                    .collect();
                report_error_with_labels(name, text, error, &located);
                noted_errors += 1;
            } else {
                analyzer_errors.push((source, error.span.into_range(), error.msg.clone()));
            }
        }
        // Warnings are non-fatal: render them, but they do not enter `errs`,
        // so they don't block codegen. They carry their own source too — an
        // unused `[must_use]` result in a module renders in that module.
        for (index, warning) in program.warnings.iter().enumerate() {
            let source = program.warning_source(index);
            load_diagnostic_file(&mut diagnostic_files, &program, source);
            let (name, text) = diagnostic_file(&diagnostic_files, source);
            report_warning(name, text, warning.span.into_range(), &warning.msg);
        }

        if emit_debug {
            write_debug(file, "analyze.out", &format!("{program:#?}"));
            // The shared graph the post-passes installed — the `-d` dump
            // describes what the compiler actually used, not a lookalike.
            write_debug(
                file,
                "callgraph.out",
                &program.call_graph().debug_dump(&program),
            );
        }

        // Never emit from a recovered tree — `check` reaches here with one, and
        // codegen over a tree with statements missing would describe a program
        // nobody wrote. (`clean` below already refuses to RETURN the output; this
        // is what stops it being produced, and with it every transformer panic a
        // salvaged tree could provoke.)
        if analyzer_errors.is_empty()
            && noted_errors == 0
            && repeated_errors == 0
            && parse_errors.is_empty()
        {
            // `--print-chunks` (bundle-splitting.md S1): report what a split
            // build would chunk. Analysis-only — the emitted JavaScript below
            // is untouched — and gated on a clean analysis, so a failing build
            // reports its diagnostics, never a plan over a broken program.
            // The leg a split would name its chunks after — the entry's own
            // name when one asked to split, the source stem otherwise (which is
            // what `--print-chunks` measures against on a leg that has not).
            let leg_name = split
                .as_ref()
                .map(|(leg, _)| (*leg).to_string())
                .unwrap_or_else(|| {
                    file.file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                        .unwrap_or_default()
                });
            if PRINT_CHUNKS.load(std::sync::atomic::Ordering::Relaxed) {
                let chunk_plan = vilan_core::chunks::plan(&program);
                print!("{}", vilan_core::chunks::render(&chunk_plan, &filename));
                // The verdict (`bundle-splitting.md` §S3, item 5): the plan is
                // the numerator, and the denominator is what the same entry
                // weighs emitted whole — so the report measures rather than
                // quotes. Emission only, discarded; the flag stays analysis-only
                // in the sense that matters, which is that it writes nothing.
                if !chunk_plan.chunks.is_empty()
                    && let Ok(measured) = vilan_core::transform_split(&program, options, &leg_name)
                {
                    println!("  verdict: {}", measured.cost().verdict());
                }
            }
            // A `split = true` leg emits through the same walk and the same
            // rename; `transform_split` returns the eager bundle where
            // `transform` returns the whole one, plus the chunk files.
            let emitted = match split {
                // A module of a package, addressed by path: analysis is the
                // whole job (E113). Emission's one diagnostic is the missing
                // `main` a module never had, and running the walk for it would
                // be asking a file to be a program because someone named it.
                _ if !goal.emits() => Ok(String::new()),
                Some((leg, sink)) => {
                    vilan_core::transform_split(&program, options, leg).map(|split_program| {
                        // Splitting is not free, and below a few KB of
                        // per-route code it is a NET LOSS on first load (S2's
                        // measurement). Say so, with this leg's own numbers,
                        // rather than leaving the author to discover it.
                        let cost = split_program.cost();
                        if cost.is_a_loss() {
                            eprintln!(
                                "{} `split` on `{leg}`: {}. Consider dropping it, or splitting a \
                                 leg with more per-route code (`vilan build --print-chunks` \
                                 reports what each route would carry)",
                                paint::warning_prefix(),
                                cost.verdict(),
                            );
                        }
                        sink.extend(split_program.chunks);
                        split_program.main
                    })
                }
                None => transform(&program, options),
            };
            match emitted {
                // The leg's source set — each path paired with the content
                // hash it was COMPILED from — which the watch loop verifies
                // (by re-hashing, never by mtime) to skip a leg whose sources
                // didn't change (backlog E12, half b).
                Ok(javascript) => {
                    let explained = resolve_const_facts(&mut diagnostic_files, &program);
                    output =
                        Some(Compiled {
                            javascript,
                            explain: explained,
                            assets: program.const_assets.clone(),
                            bundled: program.const_bundled_files.clone(),
                            sources: program
                                .sources
                                .iter()
                                .cloned()
                                .zip(program.source_hashes.iter().copied())
                                // `const asset::read` inputs — and `asset::bundle`'s
                                // files, which record through the same channel — are
                                // sources to the skip decision too: the leg
                                // recompiles when one changes, which is what recopies
                                // an edited asset under `--watch`. A missing input
                                // (`None` hash) cannot reach here: a missed read, or
                                // a bundle of a file that is not there, fails the
                                // compile.
                                .chain(program.const_input_files.iter().filter_map(
                                    |(path, hash)| hash.map(|hash| (path.clone(), hash)),
                                ))
                                .collect::<Vec<_>>(),
                        })
                }
                Err(error) => {
                    overlay_diagnostics.push(hmr::OverlayDiagnostic::located(
                        &filename,
                        source_ref,
                        error.span.into_range(),
                        error.msg.clone(),
                        error.note.as_ref().map(|note| note.msg.clone()),
                    ));
                    // The entry, and not as a fallback (E16's leftover, resolved
                    // by looking): `transform` has exactly ONE failure —
                    // `transform_entry_ast`'s missing `main`
                    // (`transformer.rs`) — and it is STRUCTURAL. Its subject is
                    // the ABSENCE of a definition, so there is no span to take a
                    // source from (it carries `0..0` for that reason), and the
                    // entity whose absence it reports is the entry's `main`. The
                    // span-source rule the post-analyze passes follow needs a
                    // span that indexes a file; this one indexes nothing, and
                    // the entry is where the missing definition was looked for.
                    // A future transformer error WITH a real span must attribute
                    // through `program.source_of(..)` like everything else.
                    analyzer_errors.push((SourceId(0), error.span.into_range(), error.msg));
                }
            }
        }
    }

    let clean = analyzer_errors.is_empty()
        && parse_errors.is_empty()
        && noted_errors == 0
        && repeated_errors == 0;
    // The overlay's copy of this leg's diagnostics (hmr.md §§2/§6): the analyzer/
    // codegen items captured above, plus the parse errors rendered with the SAME
    // `render` the terminal `report` uses — only the location prefix and framing
    // are added here. Assembled only when a caller asked for it and the build
    // failed.
    if let Some(sink) = overlay
        && !clean
    {
        for error in &parse_errors {
            // The entry's own parse errors: located in the entry (a module
            // that fails to parse reports through the analyzer path above,
            // carrying its own source).
            overlay_diagnostics.push(hmr::OverlayDiagnostic::located(
                &filename,
                source_ref,
                error.span.into_range(),
                vilan_core::parsing::render(error),
                None,
            ));
        }
        *sink = hmr::render_overlay(&filename, &overlay_diagnostics, hmr::OVERLAY_DIAGNOSTIC_CAP);
    }
    // A CASCADE of "…'s definition did not compile" is almost never several
    // broken macros (tracker N56). It is one `std` that does not match the
    // program: every derive is expanded against the world that std defines, so a
    // mismatched one fails all of them at once, and the screen fills with a
    // repeated message about the user's own `#[derive]`s. The path is the fact
    // that answers it and nothing else printed carries it — std resolution is
    // silent by design — so it is stated once, beneath the diagnostics.
    let cascade = analyzer_errors
        .iter()
        .filter(|(_, _, message)| message.ends_with("'s definition did not compile"))
        .count();
    // The entry's parse errors belong to the entry; the analyzer's carry their
    // own source.
    report(&diagnostic_files, analyzer_errors, parse_errors);
    if cascade > 1 {
        eprintln!(
            "{} {cascade} macro definitions failed to compile; this compile \
             resolved `std` from {}",
            paint::err(paint::Style::BOLD, "note:"),
            std_directory.display()
        );
    }

    match output {
        Some(compiled) if clean => Ok(compiled),
        _ => Err(ExitCode::FAILURE),
    }
}

/// Writes `javascript` to a temp file and executes it with Node.js, propagating
/// its exit code, with stdin/stdout/stderr connected to the terminal. `args` are
/// forwarded to the program, reachable through `process::args()`. (A temp file
/// rather than piping via stdin, so the program keeps its own stdin — a piped
/// script would consume it, breaking `scan()`.)
fn run_node_script(javascript: &str, args: &[String]) -> ExitCode {
    let script = env::temp_dir().join(format!("vilan-run-{}.mjs", std::process::id()));
    if let Err(error) = fs::write(&script, javascript) {
        eprintln!(
            "{} cannot write {}: {error}",
            paint::error_prefix(),
            script.display()
        );
        return ExitCode::FAILURE;
    }
    let status = spawn_node(&script, args, None).and_then(|mut child| child.wait());
    let _ = fs::remove_file(&script);
    exit_code_of(status)
}

/// Spawns `node <script> <args...>` (optionally in `cwd`), inheriting this process's
/// stdio, and returns the child **without waiting**. `node <script>` makes the
/// program's `process.argv` `[node, script, ...args]`, so its `args()` (argv.slice(2))
/// sees exactly `args`. The caller either waits on it (`vilan run`) or holds the
/// handle to stop it on the next change (`vilan run --watch`).
///
/// Every spawned child goes through [`ManagedChild::adopt`], so on Windows it is
/// assigned to a kill-on-close Job object (`windows-support.md` §6) — a restart
/// round's kill takes the program's whole process tree, and the CLI's own death
/// reaps it. On unix the wrapper is a transparent newtype: unchanged behavior.
fn spawn_node(script: &Path, args: &[String], cwd: Option<&Path>) -> std::io::Result<ManagedChild> {
    spawn_node_with_env(script, args, cwd, &[])
}

/// The environment variable a `--watch` session sets on its Node child, and the
/// whole of `std::watch::is_watching()`'s plumbing (`dev-refresh.md` §1, and
/// §5 item 2's thin surface): no wire protocol, no socket, nothing that can
/// fail independently of the process actually starting, and nothing to clean up
/// after a crashed watcher. `is_watching()` is defined under every run and is
/// `true` only when this is set.
const WATCHING_ENV: &str = "VILAN_WATCHING";

/// [`spawn_node`] for a child a `--watch` session owns — [`WATCHING_ENV`], plus
/// whatever `extra` the round carries (the HMR round adds `VILAN_HMR_PORT` and
/// `VILAN_HMR_TOKEN`, `dev-refresh.md` §5 item 2 and backlog E93 — the plain
/// restart loop adds nothing). Both watch paths go through it; plain
/// `vilan run` does not, which is what makes `is_watching()` answer the question
/// it is named for.
fn spawn_watched_node(
    script: &Path,
    args: &[String],
    cwd: Option<&Path>,
    extra: &[(&str, String)],
) -> std::io::Result<ManagedChild> {
    let mut env: Vec<(&str, String)> = vec![(WATCHING_ENV, "1".to_string())];
    env.extend(extra.iter().map(|(name, value)| (*name, value.clone())));
    spawn_node_with_env(script, args, cwd, &env)
}

fn spawn_node_with_env(
    script: &Path,
    args: &[String],
    cwd: Option<&Path>,
    env: &[(&str, String)],
) -> std::io::Result<ManagedChild> {
    let mut command = std::process::Command::new("node");
    command.arg(script).args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    for (name, value) in env {
        command.env(name, value);
    }
    command.spawn().map(ManagedChild::adopt)
}

/// Maps a launched process's result to an `ExitCode`, reporting a launch failure.
fn exit_code_of(status: std::io::Result<std::process::ExitStatus>) -> ExitCode {
    match status {
        Ok(status) => match status.code() {
            Some(code) => ExitCode::from(code as u8),
            None => ExitCode::FAILURE, // terminated by a signal
        },
        Err(error) => {
            eprintln!(
                "{} failed to launch `node`: {error} \
                 (is Node.js installed and on your PATH?)",
                paint::error_prefix()
            );
            ExitCode::FAILURE
        }
    }
}

/// Writes a `-d` debug dump alongside the source, warning (but not failing) on IO
/// error.
fn write_debug(file: &Path, extension: &str, contents: &str) {
    let path = file.with_extension(extension);
    if fs::write(&path, contents).is_err() {
        eprintln!(
            "{} failed to write {}",
            paint::warning_prefix(),
            path.display()
        );
    }
}

/// The shared ariadne configuration for every diagnostic the CLI renders
/// (`windows-support.md` §6). Three decisions:
///
/// * **char indexing, converted at this boundary.** Compiler spans are byte
///   offsets; every one is re-expressed in char offsets by [`char_range`]
///   before ariadne sees it. `IndexType::Byte` looked like the fit, but its
///   0.6.0 renderer derives a cross-source group's `file:line:col` sub-header
///   from the label's already-converted CHAR offset and then converts it *as
///   if it were still bytes* (`write.rs`: `labels[0].char_span.start` fed to
///   `get_byte_line`), so any multibyte character earlier in the file dragged
///   the sub-header lines above the label it heads (backlog E76 — a
///   `reactive.vl:363:26` header over a line-365 label). Handing ariadne one
///   index space end to end leaves it nothing to misconvert: the header and
///   the label are derived from the same number and cannot disagree.
/// * **color follows `paint.rs`'s gate.** ariadne colors unconditionally by
///   default — it leaves the terminal check to its caller — so before this,
///   `NO_COLOR=1 vilan build broken.vl 2> file` still wrote ANSI escapes into
///   the file, contradicting the per-stream contract every other CLI line
///   obeys. Every report goes to **stderr**, so the stderr gate is the one that
///   decides.
/// * that stderr routing is itself the other half: errors used to `.print()` to
///   stdout while warnings already went to stderr *specifically* so they could
///   not corrupt `build --stdout`'s JavaScript. Errors can corrupt it just as
///   well; they join the warnings (ratified call (f)).
fn diagnostic_config() -> ariadne::Config {
    ariadne::Config::new()
        .with_index_type(ariadne::IndexType::Char)
        .with_color(paint::stderr_enabled())
}

/// Reads the file a diagnostic renders against into `files`, once per source.
/// The entry is pre-seeded by the caller; a module (or a std/library file) is
/// read back through the same `read_source` the module loader used, so the
/// spans address the same bytes. A source with no path (generated code) or an
/// unreadable file records its label with EMPTY text: the message still
/// renders, with no snippet — see [`snippet`].
fn load_diagnostic_file(
    files: &mut HashMap<SourceId, (String, String)>,
    program: &Program,
    source: SourceId,
) {
    if files.contains_key(&source) {
        return;
    }
    let entry = match program.source_path(source) {
        Some(path) => {
            let name = path.display().to_string();
            let text = vilan_core::util::read_source(path).unwrap_or_default();
            (name, text)
        }
        None => ("<generated>".to_string(), String::new()),
    };
    files.insert(source, entry);
}

/// This compile's const-channel provenance with every site resolved to
/// `file:line` — the `--explain` half of a `Compiled` (G11).
///
/// It reads the record the const pass kept (`Program::const_facts`) and adds
/// nothing to it: a `ConstFact` carries a `SourceId` and a `Span` because
/// turning those into a line needs the source TEXT, which the pass does not
/// hold. This function does, through the very map the diagnostics render
/// against — so the location `--explain` prints for a `const` site and the
/// location a const-eval *error* prints for the same site are counted from one
/// set of bytes, and cannot drift.
///
/// Empty when nobody asked: the resolution re-reads every source a fact names,
/// which is a cost a plain `vilan build` should not carry.
fn resolve_const_facts(
    files: &mut HashMap<SourceId, (String, String)>,
    program: &Program,
) -> Vec<explain::Fact> {
    if !explain::asked() {
        return Vec::new();
    }
    program
        .const_facts
        .iter()
        .map(|fact| {
            load_diagnostic_file(files, program, fact.source);
            let (name, text) = diagnostic_file(files, fact.source);
            explain::Fact {
                site: format!("{name}:{}", line_of(text, fact.span.start)),
                what: match &fact.what {
                    vilan_core::const_eval::ConstFactKind::Emitted { kind } => {
                        explain::FactKind::Emitted { kind: kind.clone() }
                    }
                    vilan_core::const_eval::ConstFactKind::Bundled { name, function, .. } => {
                        explain::FactKind::Bundled {
                            name: name.clone(),
                            function: (*function).to_string(),
                        }
                    }
                    vilan_core::const_eval::ConstFactKind::Read { path, function } => {
                        explain::FactKind::Read {
                            path: path.clone(),
                            function: (*function).to_string(),
                        }
                    }
                },
            }
        })
        .collect()
}

/// The 1-based line `offset` falls on in `text`. Counted over BYTES rather
/// than through a `str` slice, so an offset past the end or inside a codepoint
/// — a span that does not index this text, the shape [`snippet`] degrades for
/// its own reasons — lands on a line instead of panicking (backlog E16's rule:
/// a span that does not fit is a bug, and the honest degrade is never an
/// abort).
fn line_of(text: &str, offset: usize) -> usize {
    let bounded = offset.min(text.len());
    1 + text.as_bytes()[..bounded]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
}

/// The (label, text) a diagnostic renders against. Every source a diagnostic
/// named has been loaded by then; the fallback keeps an unknown one visible
/// (message, no snippet) rather than rendering it against innocent text.
fn diagnostic_file(files: &HashMap<SourceId, (String, String)>, source: SourceId) -> (&str, &str) {
    files
        .get(&source)
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .unwrap_or(("<unknown>", ""))
}

/// Whether a byte range validly indexes `text` at character boundaries — the
/// one predicate behind both halves of the E16 net, [`snippet`] and
/// [`char_range`].
fn indexes_text(text: &str, span: &std::ops::Range<usize>) -> bool {
    span.start <= span.end
        && span.end <= text.len()
        && text.is_char_boundary(span.start)
        && text.is_char_boundary(span.end)
}

/// The text a label may be drawn from: the file's own text when the span
/// actually indexes it at character boundaries, otherwise nothing.
///
/// Slicing text by a span that does not index it **panics** on a mid-codepoint
/// offset ("byte index N is not a char boundary"), which takes the compiler
/// thread down with it (backlog E16) — under `IndexType::Byte` the slice was
/// ariadne's, today it is [`char_range`]'s conversion. Attribution is what
/// makes spans fit — this is the net under it: a span that does not index this
/// text is a bug, and the honest degrade is the message without a snippet,
/// never a clamped label pointing at innocent code.
fn snippet<'a>(text: &'a str, span: &std::ops::Range<usize>) -> &'a str {
    if indexes_text(text, span) { text } else { "" }
}

/// A compiler byte range re-expressed in CHAR offsets against the text it
/// indexes — the one index space every span handed to ariadne uses (see
/// [`diagnostic_config`]). Each render site converts a span exactly once and
/// reuses the result for both the report's own location and its label, so the
/// `file:line:col` header and the underline are derived from the same number.
///
/// A span that does not index `text` comes back unchanged: [`snippet`] has
/// already degraded that label's source to empty text, against which the raw
/// offsets fail ariadne's line lookup just as they always did — the message
/// still renders, without a snippet.
fn char_range(text: &str, span: &std::ops::Range<usize>) -> std::ops::Range<usize> {
    if !indexes_text(text, span) {
        return span.clone();
    }
    let start = text[..span.start].chars().count();
    start..start + text[span.start..span.end].chars().count()
}

/// The diagnostics already RENDERED this round, keyed exactly the way the
/// module loader's two-seams dedup keys its own (E102,
/// `report_module_parse_errors`): **file, position and reason** — all three of
/// what an error *is*. Not the file alone (two modules can hold the same reason
/// at the same offset), not the position alone (two different refusals land on
/// one offset), not the reason alone (the same reason recurs down a file).
///
/// `None` — the default — means DISARMED: every diagnostic renders, which is
/// right for every single-analysis path, where nothing can repeat.
///
/// [`check_workspace`] and [`check_single`] arm it, because a multi-entry
/// package's check is several analyses of ONE source tree (B182). A module
/// every leg reaches is analyzed once per leg and produces the same errors each
/// time, so kolt's two refused fields' one report each arrived three times
/// over — the entry is the same seam the loader already deduplicates, one level
/// up. Process-global rather than threaded because the rendering sits several
/// frames below the loop that knows a round is running; scoped by
/// [`RoundReports`], so a watch round starts clean and a single-unit build
/// never consults it at all.
static RENDERED_THIS_ROUND: Mutex<Option<HashSet<(String, usize, usize, String)>>> =
    Mutex::new(None);

/// Arms [`RENDERED_THIS_ROUND`] for the lifetime of one multi-unit round.
struct RoundReports;

impl RoundReports {
    fn arm() -> Self {
        *RENDERED_THIS_ROUND
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(HashSet::new());
        RoundReports
    }
}

impl Drop for RoundReports {
    fn drop(&mut self) {
        *RENDERED_THIS_ROUND
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

/// Whether this diagnostic has not been rendered yet this round — always true
/// while the ledger is disarmed. Recording is the same call, as
/// `HashSet::insert` already answers both halves.
fn first_report_this_round(file: &str, span: &std::ops::Range<usize>, message: &str) -> bool {
    match RENDERED_THIS_ROUND
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_mut()
    {
        Some(rendered) => {
            rendered.insert((file.to_string(), span.start, span.end, message.to_string()))
        }
        None => true,
    }
}

/// Renders parser diagnostics (via the handwritten frontend's `render`) and
/// analyzer/codegen diagnostics with ariadne. Analyzer diagnostics arrive
/// pre-rendered as `(source, span, message)` — each renders against the file
/// its span indexes into, and is labeled with that file (backlog E16); parse
/// errors are the entry's own and carry the structured
/// found/expected/context/hint the renderer assembles.
fn report(
    files: &HashMap<SourceId, (String, String)>,
    analyzer_errors: Vec<(SourceId, std::ops::Range<usize>, String)>,
    parse_errors: Vec<vilan_core::parsing::ParseError>,
) {
    let diagnostics = analyzer_errors
        .into_iter()
        .chain(parse_errors.into_iter().map(|error| {
            (
                SourceId(0),
                error.span.into_range(),
                vilan_core::parsing::render(&error),
            )
        }));
    for (source, span, message) in diagnostics {
        let (filename, text) = diagnostic_file(files, source);
        let char_span = char_range(text, &span);
        Report::build(ReportKind::Error, (filename.to_string(), char_span.clone()))
            .with_config(diagnostic_config())
            .with_message(&message)
            .with_label(
                Label::new((filename.to_string(), char_span))
                    .with_message(&message)
                    .with_color(Color::Red),
            )
            .finish()
            // stderr, like the warnings (ratified call (f)): a diagnostic must
            // never land in `build --stdout`'s JavaScript.
            .eprint(sources([(
                filename.to_string(),
                snippet(text, &span).to_string(),
            )]))
            .unwrap()
    }
}

/// Renders one analyzer diagnostic that carries secondary locations: the
/// primary label at the error's span, then one label per secondary — the E78
/// requirement trace's hops (in trace order, entry → read) and/or the C3
/// note ("first call here", "generated by this attribute";
/// diagnostics-standard.md §3) — each at its own location, in its own file
/// when it lives elsewhere. Every span is converted to ariadne's char index
/// space exactly once, against its own file's text (E76's one-index-space
/// rule — the sub-header and its label derive from the same number and
/// cannot disagree); the byte span rides along for the snippet validity
/// test.
fn report_error_with_labels(
    // The primary span's own file (name, contents) — a module's error renders
    // in the module (backlog E16).
    filename: &str,
    src: &str,
    error: &vilan_core::Error,
    // Each secondary label, paired with its own file when it lives elsewhere
    // (name, contents) — cross-source labels point into std or an imported
    // module.
    located: &[(&vilan_core::error::Note, Option<(&str, &str)>)],
) {
    let primary_span = error.span.into_range();
    // Every label, addressed to the file it renders in: the primary in red,
    // every secondary — trace hop or C3 note — in the note's yellow.
    type LabelRow<'a> = (
        &'a str,
        std::ops::Range<usize>,
        std::ops::Range<usize>,
        &'a str,
        Color,
    );
    let mut labels: Vec<LabelRow> = vec![(
        filename,
        primary_span.clone(),
        char_range(src, &primary_span),
        error.msg.as_str(),
        Color::Red,
    )];
    for (secondary, file) in located {
        let byte_span = secondary.span.into_range();
        let (label_filename, char_span) = match file {
            Some((name, text)) => (*name, char_range(text, &byte_span)),
            // Same file as the primary: the span indexes the text the
            // primary already brought.
            None => (filename, char_range(src, &byte_span)),
        };
        labels.push((
            label_filename,
            byte_span,
            char_span,
            secondary.msg.as_str(),
            Color::Yellow,
        ));
    }
    // The file table ariadne slices from: each named file once, its text
    // blanked when ANY of its labels' spans fails to index it — the honest
    // degrade is the messages without snippets, never a clamped label over
    // innocent code (see [`snippet`]).
    let mut files: Vec<(String, String)> = vec![(filename.to_string(), src.to_string())];
    for (_, file) in located {
        if let Some((name, text)) = file
            && !files.iter().any(|(existing, _)| existing == name)
        {
            files.push((name.to_string(), text.to_string()));
        }
    }
    for (name, text) in &mut files {
        let blank = labels
            .iter()
            .filter(|(label_file, _, _, _, _)| label_file == name)
            .any(|(_, byte_span, _, _, _)| snippet(text, byte_span).is_empty() && !text.is_empty());
        if blank {
            text.clear();
        }
    }
    // ariadne renders labels in the order handed over, opening a NEW
    // windowed section (with a repeated sub-header) whenever a label lands
    // above an already-rendered line — so labels go over in source order,
    // grouped per file (the primary's file first), one section per file.
    // The chain's own entry → read order is the trace vector's, which the
    // language server publishes verbatim; the terminal is a spatial renderer
    // and reads best in source order. The sort is stable, so the primary
    // stays ahead of a secondary sharing its exact span.
    let file_rank = |name: &str| files.iter().position(|(existing, _)| existing == name);
    labels.sort_by_key(|(label_file, byte_span, _, _, _)| {
        (file_rank(label_file), byte_span.start, byte_span.end)
    });
    let primary_char_span = char_range(src, &primary_span);
    let mut report = Report::build(ReportKind::Error, (filename.to_string(), primary_char_span))
        .with_config(diagnostic_config())
        .with_message(error.msg.clone());
    for (label_filename, _byte_span, char_span, message, color) in labels {
        report = report.with_label(
            Label::new((label_filename.to_string(), char_span))
                .with_message(message)
                .with_color(color),
        );
    }
    report
        .finish()
        // stderr, like the warnings (ratified call (f)).
        .eprint(sources(files))
        .unwrap();
}

/// Renders a single analyzer warning (e.g. an unused `[must_use]` result) — like
/// `report`, but `ReportKind::Warning` and non-fatal. Carries its own file too.
fn report_warning(filename: &str, src: &str, span: std::ops::Range<usize>, message: &str) {
    let char_span = char_range(src, &span);
    Report::build(
        ReportKind::Warning,
        (filename.to_string(), char_span.clone()),
    )
    .with_config(diagnostic_config())
    .with_message(message)
    .with_label(
        Label::new((filename.to_string(), char_span))
            .with_message(message)
            .with_color(Color::Yellow),
    )
    .finish()
    // stderr, so it doesn't corrupt `build --stdout` JS — the call the
    // errors now match too.
    .eprint(sources([(
        filename.to_string(),
        snippet(src, &span).to_string(),
    )]))
    .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Renamed CLI spellings (proposal/deprecation.md §4) -----------------

    /// A SYNTHETIC rename pair — no real rename exists, and the pins must not
    /// invent one on the real surface — in exactly the shape a real rename
    /// declares: the new spelling as an ordinary arg, the old as a HIDDEN one.
    #[derive(clap::Parser)]
    struct RenameProbe {
        #[arg(long)]
        fresh_spelling: Option<String>,
        /// The deprecated old spelling: still parsed, hidden from help,
        /// folded into `fresh_spelling` at dispatch with the warning.
        #[arg(long, hide = true)]
        stale_spelling: Option<String>,
    }

    fn reconcile(probe: RenameProbe) -> Result<(Option<String>, Option<String>), String> {
        reconcile_renamed_flag(
            probe.fresh_spelling,
            probe.stale_spelling,
            "--fresh-spelling",
            "--stale-spelling",
        )
    }

    #[test]
    fn an_old_spelling_still_parses_and_warns_into_the_new() {
        let probe = RenameProbe::parse_from(["probe", "--stale-spelling", "value"]);
        let (value, warning) = reconcile(probe).expect("the old spelling alone reconciles");
        assert_eq!(value.as_deref(), Some("value"));
        assert_eq!(
            warning.as_deref(),
            Some("`--stale-spelling` is deprecated; use `--fresh-spelling`"),
            "the ledger row 247 head, verbatim"
        );
    }

    #[test]
    fn the_new_spelling_is_silent_and_the_old_is_hidden_from_help() {
        let probe = RenameProbe::parse_from(["probe", "--fresh-spelling", "value"]);
        let (value, warning) = reconcile(probe).expect("the new spelling reconciles");
        assert_eq!(value.as_deref(), Some("value"));
        assert!(warning.is_none(), "the new spelling never warns");
        // `hide = true` holds: help offers only the new spelling, which is
        // what makes the old one an alias in its window rather than surface.
        use clap::CommandFactory as _;
        let help = RenameProbe::command().render_long_help().to_string();
        assert!(help.contains("--fresh-spelling"), "{help}");
        assert!(!help.contains("--stale-spelling"), "{help}");
    }

    #[test]
    fn both_spellings_conflicting_refuse_and_agreeing_fold() {
        let probe =
            RenameProbe::parse_from(["probe", "--fresh-spelling", "a", "--stale-spelling", "b"]);
        let error = reconcile(probe).expect_err("conflicting values must refuse");
        assert!(
            error.contains(
                "`--stale-spelling` is a deprecated spelling of `--fresh-spelling`, and both \
                 are given with different values; drop `--stale-spelling`"
            ),
            "{error}"
        );
        // Both spellings AGREEING fold into one value — still warning, so the
        // author is steered off the old spelling either way.
        let probe =
            RenameProbe::parse_from(["probe", "--fresh-spelling", "a", "--stale-spelling", "a"]);
        let (value, warning) = reconcile(probe).expect("agreeing values fold");
        assert_eq!(value.as_deref(), Some("a"));
        assert!(warning.is_some(), "the old spelling warns even when folded");
    }

    // --- The entry file's exact case (windows-support.md §5 / §12) ----------
    //
    // Pinned at the checker, not end to end: on a case-SENSITIVE filesystem a
    // wrong-case entry never opens, so `compile_to_js` fails at the read long
    // before the check runs (the same reason the module arm is CI's to prove —
    // see `module_paths.rs`). The windows-latest CI leg is the mismatch-arm
    // e2e; the happy path is pinned there on every platform.

    /// A fresh, empty scratch directory for one entry-case test.
    fn entry_scratch(tag: &str) -> PathBuf {
        let directory = env::temp_dir().join(format!(
            "vilan-entry-case-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("create the scratch directory");
        directory
    }

    #[test]
    fn an_exact_case_entry_is_no_mismatch() {
        let root = entry_scratch("exact");
        fs::write(root.join("main.vl"), "").expect("write main.vl");
        assert_eq!(entry_case_mismatch(&root.join("main.vl"), &root), None);
        // The false-positive guard, and an arm only a case-SENSITIVE
        // filesystem can hold: with both spellings genuinely on disk, the
        // requested one matches exactly and the near-miss sibling is not
        // reported. On a case-insensitive filesystem the second write lands in
        // the FIRST file (one directory entry — the windows CI leg proved it:
        // the checker then correctly reports the requested spelling against
        // the surviving entry), so the arm is gated on a runtime probe of what
        // this filesystem actually did, not on a platform guess.
        fs::write(root.join("Main.vl"), "").expect("write Main.vl");
        let distinct = fs::read_dir(&root)
            .expect("read the scratch dir")
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("main.vl")
            })
            .count();
        if distinct == 2 {
            assert_eq!(entry_case_mismatch(&root.join("main.vl"), &root), None);
            assert_eq!(entry_case_mismatch(&root.join("Main.vl"), &root), None);
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_wrong_case_entry_names_both_spellings() {
        // `[package] entry = "Main.vl"` against a `main.vl` on disk — what NTFS
        // resolves happily and Linux refuses.
        let root = entry_scratch("wrong");
        fs::write(root.join("main.vl"), "").expect("write main.vl");
        assert_eq!(
            entry_case_mismatch(&root.join("Main.vl"), &root),
            Some(("Main.vl".to_string(), "main.vl".to_string()))
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_nested_entry_path_has_its_directory_checked_too() {
        // `[entry.<name>] path = "Web/client.vl"`: the directory component is
        // part of the manifest's spelling, so it counts exactly as a module
        // directory's does.
        let root = entry_scratch("nested");
        fs::create_dir_all(root.join("web")).expect("create web/");
        fs::write(root.join("web").join("client.vl"), "").expect("write client.vl");
        assert_eq!(
            entry_case_mismatch(&root.join("Web").join("client.vl"), &root),
            Some(("Web".to_string(), "web".to_string()))
        );
        // …and the file under a correctly-spelled directory.
        assert_eq!(
            entry_case_mismatch(&root.join("web").join("Client.vl"), &root),
            Some(("Client.vl".to_string(), "client.vl".to_string()))
        );
        // The exact spelling of both is silent.
        assert_eq!(
            entry_case_mismatch(&root.join("web").join("client.vl"), &root),
            None
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_bare_filename_entry_is_checked_against_the_working_directory() {
        // `vilan build Main.vl` — `pkg_root_of` gives `.`, which `strip_prefix`
        // cannot cancel, so the fallback arm is what has to answer. Exercised by
        // building the same shape with an absolute root that the entry does NOT
        // nest under, which takes the identical branch without moving the
        // process's working directory (tests share one).
        let root = entry_scratch("bare");
        fs::write(root.join("main.vl"), "").expect("write main.vl");
        let elsewhere = root.join("not-the-parent");
        assert_eq!(
            entry_case_mismatch(&root.join("Main.vl"), &elsewhere),
            Some(("Main.vl".to_string(), "main.vl".to_string()))
        );
        assert_eq!(entry_case_mismatch(&root.join("main.vl"), &elsewhere), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_absent_entry_is_not_a_case_mismatch() {
        // A missing entry is the read's failure to report, not this check's —
        // and `compile_to_js` never reaches the check in that case anyway.
        let root = entry_scratch("absent");
        assert_eq!(entry_case_mismatch(&root.join("main.vl"), &root), None);
        let _ = fs::remove_dir_all(&root);
    }

    // --- A15's follow-up: the manifest-designated default `run` entry -------

    /// A Node/browser build unit named `name`, enough for `select_node_entry`.
    fn member(name: &str, platform: Platform) -> (Unit, Platform) {
        (
            Unit {
                name: name.to_string(),
                entry: PathBuf::from(format!("src/{name}.vl")),
                pkg_root: PathBuf::from("src"),
                package_dir: None,
                split: false,
                options: BuildOptions::default(),
                platform_reasons: Vec::new(),
                entry_mode: vilan_core::EntryMode::Declared,
            },
            platform,
        )
    }

    fn node_platform() -> Platform {
        Platform::parse("node").expect("`node` is a platform")
    }

    #[test]
    fn a_designated_default_entry_picks_the_leg_without_a_flag() {
        let members = vec![
            member("server", node_platform()),
            member("worker", node_platform()),
            member("client", Platform::Browser),
        ];
        let designated = DefaultEntry::new("[package] default-entry", Some("worker"));
        let chosen = select_node_entry(&members, None, &designated).expect("a clean choice");
        assert_eq!(chosen.map(|unit| unit.name.as_str()), Some("worker"));
    }

    #[test]
    fn the_entry_flag_overrides_the_designated_default() {
        let members = vec![
            member("server", node_platform()),
            member("worker", node_platform()),
        ];
        let designated = DefaultEntry::new("[project] default-entry", Some("worker"));
        let chosen =
            select_node_entry(&members, Some("server"), &designated).expect("a clean choice");
        assert_eq!(chosen.map(|unit| unit.name.as_str()), Some("server"));
    }

    #[test]
    fn a_designated_entry_that_is_not_a_node_leg_names_the_manifest_key() {
        // A typo, or a designation pointing at the browser leg: the message
        // quotes the key the user wrote, not the flag they didn't.
        let members = vec![
            member("server", node_platform()),
            member("client", Platform::Browser),
        ];
        let designated = DefaultEntry::new("[project] default-entry", Some("client"));
        let error = match select_node_entry(&members, None, &designated) {
            Err(error) => error,
            Ok(_) => panic!("a browser leg is not a runnable node entry"),
        };
        assert!(
            error.contains("`[project] default-entry = \"client\"`")
                && error.contains("candidates: server"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn the_ambiguity_error_offers_both_the_flag_and_the_manifest_key() {
        // 2+ Node legs with nothing designating one: the error now teaches the
        // permanent fix alongside the one-off flag.
        let members = vec![
            member("server", node_platform()),
            member("worker", node_platform()),
        ];
        let undesignated = DefaultEntry::new("[package] default-entry", None);
        let error = match select_node_entry(&members, None, &undesignated) {
            Err(error) => error,
            Ok(_) => panic!("two node legs with no designation is ambiguous"),
        };
        assert!(
            error.contains("--entry <name>")
                && error.contains("`[package] default-entry`")
                && error.contains("server, worker"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn a_lone_node_leg_still_needs_no_designation() {
        // The undesignated single-leg path is unchanged — the common shape pays
        // nothing for the new key.
        let members = vec![
            member("server", node_platform()),
            member("client", Platform::Browser),
        ];
        let undesignated = DefaultEntry::new("[project] default-entry", None);
        let chosen = select_node_entry(&members, None, &undesignated).expect("a clean choice");
        assert_eq!(chosen.map(|unit| unit.name.as_str()), Some("server"));
    }

    #[test]
    fn watch_roots_from_a_file_is_its_parent_directory() {
        // A non-existent `.vl` path isn't a directory, so it resolves to its parent —
        // where its `pkg::` siblings live.
        let roots = watch_roots(&Some(PathBuf::from("project/src/main.vl")));
        assert_eq!(roots, vec![PathBuf::from("project/src")]);
    }

    #[test]
    fn watch_roots_from_a_directory_is_the_directory() {
        // A real directory (so `is_dir()` holds) is watched as-is.
        let dir = env::temp_dir();
        assert_eq!(watch_roots(&Some(dir.clone())), vec![dir]);
    }

    #[test]
    fn scan_vl_tracks_only_vl_files_and_sees_additions() {
        let dir = env::temp_dir().join(format!("vilan-watch-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.vl"), "fun main() {}\n").unwrap();
        // A build's own output — must never be watched (else it triggers itself).
        fs::write(dir.join("a.js"), "// generated\n").unwrap();
        let roots = vec![dir.clone()];

        let snapshot = scan_vl(&roots);
        assert!(
            snapshot.keys().any(|path| path.ends_with("a.vl")),
            "the `.vl` source must be tracked"
        );
        assert!(
            !snapshot.keys().any(|path| path.ends_with("a.js")),
            "generated `.js` must not be tracked"
        );

        // Adding a `.vl` file changes the snapshot — a rebuild trigger.
        fs::write(dir.join("b.vl"), "fun helper() {}\n").unwrap();
        assert_ne!(scan_vl(&roots), snapshot);
        // Adding a `.js` file does not.
        let after_js = scan_vl(&roots);
        fs::write(dir.join("c.js"), "// also generated\n").unwrap();
        assert_eq!(scan_vl(&roots), after_js, "a new `.js` is not a change");
    }

    #[test]
    fn watch_snapshot_includes_recorded_const_inputs() {
        // `const asset::read` inputs are build inputs (K13 step 2): once a
        // compile records one, the watcher polls it beside the `.vl` scan —
        // and a recorded-but-missing input joins the snapshot the moment it
        // APPEARS, which is the round trigger for "the file the build wanted
        // now exists".
        let dir = env::temp_dir().join(format!("vilan-watch-input-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let present = dir.join("page.md");
        let missing = dir.join("not-yet.md");
        fs::write(&present, "# page\n").unwrap();
        record_const_inputs(&[(present.clone(), Some(1)), (missing.clone(), None)]);
        let roots = vec![dir.clone()];
        let snapshot = watch_snapshot(&roots);
        assert!(
            snapshot.contains_key(&present),
            "a recorded input joins the watched set: {snapshot:?}"
        );
        assert!(
            !snapshot.contains_key(&missing),
            "a missing input stays out until it appears"
        );
        fs::write(&missing, "# here now\n").unwrap();
        let next = watch_snapshot(&roots);
        assert!(
            next.contains_key(&missing),
            "an appearing input is a snapshot change"
        );
        assert_ne!(next, snapshot);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn watch_snapshot_expands_a_recorded_directory_input_to_its_tree() {
        // G10: a declared directory input means its TREE, because that is what
        // the freshness stamp already reads it as. The tree alone would miss an
        // edit to a file inside it — the directory's own mtime does not move
        // for that — and the directory alone would miss its own appearance
        // (N30), so the snapshot carries both.
        let dir = env::temp_dir().join(format!("vilan-watch-tree-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("icons/svg")).unwrap();
        let nested = dir.join("icons/svg/check.svg");
        fs::write(&nested, "<svg/>\n").unwrap();
        record_watched_inputs([(dir.join("icons"), InputReading::Tree)]);
        let roots = vec![dir.clone()];

        let snapshot = watch_snapshot(&roots);
        assert!(
            snapshot.contains_key(&nested),
            "a file nested under a declared directory is watched: {snapshot:?}"
        );
        assert!(
            snapshot.contains_key(&dir.join("icons")),
            "the declared directory is an entry in its own right: {snapshot:?}"
        );
        assert!(
            snapshot.contains_key(&dir.join("icons/svg")),
            "and so is a directory nested inside it: {snapshot:?}"
        );

        // Adding a file under the tree is a snapshot difference: the round
        // trigger the lucide case needed.
        fs::write(dir.join("icons/svg/x.svg"), "<svg/>\n").unwrap();
        assert_ne!(watch_snapshot(&roots), snapshot);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn watch_snapshot_sees_a_recorded_directory_that_appears_empty() {
        // N30: `asset::read_dir` on a missing directory records the miss, and
        // creating that directory — even with nothing in it — is the change
        // that makes the failed compile succeed. Expanding the directory to its
        // FILES alone said nothing about an empty one, so the appearance added
        // no entry and started no round; the first file created inside it did.
        let dir = env::temp_dir().join(format!("vilan-watch-empty-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let icons = dir.join("icons");
        record_const_inputs(&[(icons.clone(), None)]);
        let roots = vec![dir.clone()];

        let missing = watch_snapshot(&roots);
        assert!(
            !missing.contains_key(&icons),
            "a recorded-missing directory stays out until it appears"
        );

        fs::create_dir(&icons).unwrap();
        let appeared = watch_snapshot(&roots);
        assert!(
            appeared.contains_key(&icons),
            "an EMPTY directory appearing is a snapshot change: {appeared:?}"
        );
        assert_ne!(appeared, missing);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_const_listed_directory_is_watched_by_its_membership_not_its_tree() {
        // G21 (audit run 6's F23). `asset::read_dir` keys a directory on its
        // IMMEDIATE NAMES, so the watcher must wake for a name appearing or
        // vanishing there and for nothing else. Expanding it to the whole tree
        // — the hook declaration's reading — woke rounds the const key provably
        // did not move for: a listed file's CONTENT, and any file deeper in.
        //
        // The rounds were harmless (the leg re-keys the directory, finds it
        // unchanged and skips), which is why this sat unnoticed; the defect is
        // that the watcher read a declaration differently from the gate it woke.
        let dir = env::temp_dir().join(format!("vilan-watch-listing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("icons/svg")).unwrap();
        let listed = dir.join("icons/check.svg");
        let nested = dir.join("icons/svg/deep.svg");
        fs::write(&listed, "<svg/>\n").unwrap();
        fs::write(&nested, "<svg/>\n").unwrap();
        record_const_inputs(&[(dir.join("icons"), Some(1))]);
        let roots = vec![dir.clone()];
        // [`RECORDED_INPUT_PATHS`] is process-global and the tests around this
        // one record into it in parallel, so a whole-map comparison would be
        // reading their fixtures. Compare this fixture's own rows.
        let mine = |roots: &[PathBuf]| -> BTreeMap<PathBuf, SystemTime> {
            watch_snapshot(roots)
                .into_iter()
                .filter(|(path, _)| path.starts_with(&dir))
                .collect()
        };

        let snapshot = mine(&roots);
        assert!(
            snapshot.contains_key(&dir.join("icons")),
            "the listed directory is watched in its own right (N30): {snapshot:?}"
        );
        assert!(
            !snapshot.contains_key(&listed),
            "but a file it merely LISTED is not — the listing read its name, \
             never its bytes: {snapshot:?}"
        );
        assert!(
            !snapshot.contains_key(&nested),
            "and neither is a file the listing never named at all: {snapshot:?}"
        );

        // Editing a listed file's content moves no key the const gate holds.
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(&listed, "<svg viewBox=\"0 0 1 1\"/>\n").unwrap();
        assert_eq!(
            mine(&roots),
            snapshot,
            "an edit inside a listed directory is not a change to its listing"
        );

        // A name appearing IS, and it reaches the snapshot through the
        // directory's own mtime — which is the same thing the listing key is a
        // function of.
        fs::write(dir.join("icons/x.svg"), "<svg/>\n").unwrap();
        assert_ne!(
            mine(&roots),
            snapshot,
            "a file appearing in the listed directory starts a round"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_path_recorded_under_both_readings_keeps_the_tree() {
        // The overlap, ruled: a directory a hook DECLARES and the const channel
        // also lists is read as a tree, because the tree is the superset and the
        // safe direction here is the spurious round rather than the missed one —
        // the hook's freshness gate really does depend on every byte inside.
        let dir = env::temp_dir().join(format!("vilan-watch-both-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("shared")).unwrap();
        let inside = dir.join("shared/a.txt");
        fs::write(&inside, "one\n").unwrap();
        record_const_inputs(&[(dir.join("shared"), Some(1))]);
        record_watched_inputs([(dir.join("shared"), InputReading::Tree)]);

        let snapshot = watch_snapshot(std::slice::from_ref(&dir));
        assert!(
            snapshot.contains_key(&inside),
            "the tree reading wins whichever order the two recorders ran in: \
             {snapshot:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hooks_declared_inputs_are_recorded_and_its_outputs_are_not() {
        // The wake-up set is the declaration, resolved against the manifest's
        // own directory — and it stops at `inputs`: recording an output would
        // let a hook wake the loop that ran it.
        let dir = env::temp_dir().join(format!("vilan-watch-hook-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("icons.lock");
        let output = dir.join("generated.vl.txt");
        fs::write(&input, "lock\n").unwrap();
        fs::write(&output, "generated\n").unwrap();
        let hooks = BuildHooks {
            dir: dir.clone(),
            commands: Vec::new(),
            declared: vec![DeclaredHook {
                name: "icons".to_string(),
                commands: vec!["true".to_string()],
                inputs: vec!["icons.lock".to_string()],
                outputs: vec!["generated.vl.txt".to_string()],
            }],
        };
        hooks.record_watched_inputs();

        let snapshot = watch_snapshot(std::slice::from_ref(&dir));
        assert!(
            snapshot.contains_key(&input),
            "a declared input joins the watched set: {snapshot:?}"
        );
        assert!(
            !snapshot.contains_key(&output),
            "a declared output must never be watched — that is the build \
             triggering itself"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    // The `run --watch` temp script used to be rewritten every round and never
    // removed: a temp-directory leak everywhere, and an intermittent sharing
    // violation on Windows, which has no unlink-while-open
    // (windows-support.md §5). The restart round removes it once the child that
    // was executing it is killed and reaped; the loop is interactive, so what is
    // pinnable in-process is the cleanup helper itself.
    #[test]
    fn the_watch_script_is_removed_and_a_missing_one_is_not_an_error() {
        let script = watch_script_path();
        assert!(
            script.starts_with(env::temp_dir()),
            "the round's script lives in the temp directory: {}",
            script.display()
        );
        fs::write(&script, "// a round's compiled program\n").unwrap();
        assert!(script.exists());
        remove_watch_script();
        assert!(!script.exists(), "the round's script must not survive it");
        // Idempotent: the loop-exit call runs after the round already removed it.
        remove_watch_script();
        assert!(!script.exists());
    }

    // The net under the attribution (backlog E16). ariadne slices the source by
    // the label's byte range and PANICS on a mid-codepoint index, killing the
    // compiler thread; a span that does not index the text it was handed loses
    // its snippet instead — the message still prints, with its file.
    #[test]
    fn a_span_that_indexes_the_text_keeps_its_snippet() {
        let text = "let x = 1;\n";
        assert_eq!(snippet(text, &(4..5)), text);
        // The boundaries themselves index it: an empty span at the end is fine.
        assert_eq!(snippet(text, &(text.len()..text.len())), text);
    }

    #[test]
    fn a_span_that_does_not_index_the_text_loses_its_snippet() {
        // Mid-codepoint — the fatal case: `é` occupies bytes 4..6.
        let multibyte = "let é = 1;\n";
        assert!(!multibyte.is_char_boundary(5));
        assert_eq!(snippet(multibyte, &(5..7)), "");
        // Past the end — a span from a longer file.
        assert_eq!(snippet(multibyte, &(400..420)), "");
        // Inverted — never produced, never sliced either. The range is
        // DELIBERATELY reversed (this pins that `snippet` tolerates one, not
        // that anyone would write `6..4` by accident), so clippy's "probably a
        // mistake" lint is silenced for this one assertion rather than fixed
        // away. (The allow needs its own block: clippy ignores an attribute
        // placed directly on a macro-invocation statement.)
        #[allow(clippy::reversed_empty_ranges)]
        {
            assert_eq!(snippet(multibyte, &(6..4)), "");
        }
    }

    // --- The one index space handed to ariadne (backlog E76) ----------------
    //
    // `char_range` re-expresses a compiler byte span in char offsets against
    // the text it indexes; both the report location and every label are built
    // from the converted range, so a header and the label under it cannot
    // name different positions. The end-to-end agreement is pinned in
    // `tests/diagnostics.rs`; these pin the conversion itself.

    #[test]
    fn an_ascii_span_converts_to_identical_char_offsets() {
        // ASCII: byte and char offsets coincide, so nothing may move.
        assert_eq!(char_range("let x = 1;\n", &(4..5)), 4..5);
    }

    #[test]
    fn multibyte_text_before_a_span_shrinks_its_char_offsets() {
        // `é` (2 bytes) and `—` (3 bytes) precede `x`: byte 5..6 is char 2..3.
        // This is the divergence that dragged a cross-source sub-header lines
        // above its label when the byte offset was read as an index ariadne
        // could convert a second time.
        let text = "é—x = 1;\n";
        assert_eq!(&text[5..6], "x");
        assert_eq!(char_range(text, &(5..6)), 2..3);
        // A span STRADDLING multibyte text shrinks in length too.
        assert_eq!(char_range(text, &(0..6)), 0..3);
    }

    #[test]
    fn an_empty_span_at_the_end_of_text_converts_to_the_char_end() {
        // The EOF parse-error shape: empty, at the very end.
        let text = "é\n";
        assert_eq!(char_range(text, &(3..3)), 2..2);
    }

    #[test]
    fn a_span_that_does_not_index_the_text_converts_unchanged() {
        // The degrade contract shared with `snippet`: the raw offsets come
        // back untouched and fail ariadne's line lookup against the empty
        // snippet text — message without a snippet, never a shifted label.
        let multibyte = "let é = 1;\n";
        assert_eq!(char_range(multibyte, &(5..7)), 5..7); // mid-codepoint
        assert_eq!(char_range(multibyte, &(400..420)), 400..420); // past the end
        assert_eq!(char_range("", &(5..7)), 5..7); // and against empty text
    }
}
