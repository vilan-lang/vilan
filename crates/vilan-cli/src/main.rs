use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, SystemTime},
};

use ariadne::{Color, Label, Report, ReportKind, sources};
use clap::{Parser as _, Subcommand};
mod hmr;
mod init;
mod job;
mod paint;
mod upgrade;

use job::ManagedChild;
use vilan_core::analyzer::{Program, SourceId, analyze, check_library_contract};
use vilan_core::async_infer;
use vilan_core::call_graph::CallGraph;
use vilan_core::context;
use vilan_core::manifest::Package;
use vilan_core::transformer::transform;
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
        /// Also emit `.parse.out` / `.analyze.out` / `.callgraph.out` debug dumps.
        #[arg(short, long)]
        debug: bool,
        /// Rebuild whenever a watched `.vl` source file changes (Ctrl-C to stop).
        #[arg(long)]
        watch: bool,
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
        /// Also emit `.parse.out` / `.analyze.out` / `.callgraph.out` debug dumps.
        #[arg(short, long)]
        debug: bool,
        /// Re-check whenever a watched `.vl` source file changes (Ctrl-C to stop).
        #[arg(long)]
        watch: bool,
    },
    /// Build and run a source file, forwarding any trailing arguments to the
    /// program (reach them with `process::args()`).
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
    /// Update this binary (and `vilan-lsp` beside it) to the newest release.
    /// This is the only command that touches the network.
    Upgrade {
        /// Report whether a newer release exists without changing anything.
        #[arg(long)]
        check: bool,
    },
}

fn main() -> ExitCode {
    // Compilation recurses over deeply-nested ASTs and type graphs (e.g. closures
    // stored in data structures plus generic monomorphization), which can run
    // past the default main-thread stack on otherwise-valid programs. Do the work
    // on a worker with a generous stack, as rustc and other compilers do; the
    // reservation is virtual address space, so it costs nothing unless used.
    const COMPILER_STACK_SIZE: usize = 256 * 1024 * 1024;
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
        } => match effective_backend(backend.as_deref()) {
            Err(message) => report_error(&message),
            Ok(_backend) => {
                PRINT_CHUNKS.store(print_chunks, std::sync::atomic::Ordering::Relaxed);
                let roots = watch.then(|| watch_roots(&file));
                run_or_watch(roots, move || {
                    build_once(file.clone(), stdout, platform.clone(), debug)
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
) -> ExitCode {
    with_project(file, |project| {
        if let Err(code) = run_build_hooks(&project) {
            return code;
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
            Project::Workspace { root, members, .. } => build_workspace(&root, &members, debug),
            Project::Library { name, .. } => not_buildable_library(&name),
        }
    })
}

/// Runs the project's `[build] run` hooks before it is built (A9), reporting
/// and failing on the first that fails. `vilan check` deliberately doesn't call
/// this: it produces no artifacts, so there is nothing for a hook to feed.
fn run_build_hooks(project: &Project) -> Result<(), ExitCode> {
    let Some(hooks) = project.hooks() else {
        return Ok(());
    };
    hooks.run().map_err(|message| {
        eprintln!("{} {message}", paint::error_prefix());
        ExitCode::FAILURE
    })
}

/// Type-checks the project once. A standalone `[library]` has no fixed platform, so
/// it verifies the platform contract (§4.2) instead of a single-platform build.
fn check_once(file: Option<PathBuf>, platform: Option<String>, debug: bool) -> ExitCode {
    with_project(file, |project| match project {
        Project::Single {
            unit,
            platform: package_platform,
            ..
        } => match effective_platform(platform.as_deref(), package_platform) {
            // A `none` package is a pure library — not buildable, but type-checkable
            // (against the base layer only).
            Ok(platform) => check_single(&unit, platform, debug),
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
        if let Err(code) = run_build_hooks(&project) {
            return code;
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
            Project::Library { name, .. } => not_buildable_library(&name),
        }
    })
}

// --- `--watch` mode (roadmap P5) --------------------------------------------

/// How often the watcher polls for changes.
const WATCH_POLL_INTERVAL: Duration = Duration::from_millis(300);

/// Runs `action` once and returns its exit code (no `--watch`, `roots` is `None`),
/// or — under `--watch` — re-runs it on every change to a `.vl` file under `roots`.
fn run_or_watch(roots: Option<Vec<PathBuf>>, mut action: impl FnMut() -> ExitCode) -> ExitCode {
    match roots {
        None => action(),
        Some(roots) => watch_loop(&roots, move || {
            let _ = action();
        }),
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
        collect_vl_files(root, &mut files);
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
/// temp script (`vilan-watch-<pid>.js`) outlives it — one leaked file per watch
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
fn watch_loop(roots: &[PathBuf], mut action: impl FnMut()) -> ExitCode {
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
    eprintln!(
        "{}",
        paint::err(
            paint::Style::CYAN,
            &format!("[watch] watching {watched} for `.vl` changes (Ctrl-C to stop)")
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
    let mut snapshot = scan_vl(roots);
    action();
    loop {
        std::thread::sleep(WATCH_POLL_INTERVAL);
        let next = scan_vl(roots);
        if next != snapshot {
            snapshot = next;
            eprintln!(
                "\n{}",
                paint::err(paint::Style::CYAN, "[watch] change detected, re-running")
            );
            action();
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
    let code = watch_loop(&roots, move || match &channel {
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
            // The plain restart loop recompiles and respawns wholesale, so the
            // per-leg skip doesn't drop in naturally here (there are no retained
            // per-leg artifacts to reuse) (backlog E12).
            if let Some(mut previous) = child.take() {
                let _ = previous.kill();
                let _ = previous.wait();
                // The child is reaped, so nothing holds the round's temp script
                // any more and it can be removed before the next one writes it.
                remove_watch_script();
            }
            child = build_and_spawn_run(file.clone(), &args, entry.as_deref());
        }
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
    if let Err(message) = hooks.run() {
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
        // Read the same way the compiler reads (BOM dropped,
        // windows-support.md §2), or the hash recorded from the text it
        // consumed could never match.
        vilan_core::util::read_source(path)
            .ok()
            .map(|text| vilan_core::content_hash(&text))
    };
    let skip: BTreeSet<String> = if force_full {
        BTreeSet::new()
    } else {
        members
            .iter()
            .filter(|(_, platform)| !platform.is_none())
            .filter_map(|(unit, _)| {
                let previous = state.legs.iter().find(|leg| leg.name == unit.name)?;
                hmr::leg_is_current(&previous.sources, &current_hash).then(|| unit.name.clone())
            })
            .collect()
    };

    // Compile every host leg (skipped legs excepted), capturing the RAW bundle
    // bytes (before the shim is prepended — the shim embeds the version, so
    // shim-inclusive bytes would differ every round and misclassify everything
    // as a swap).
    let mut next = Vec::new();
    let mut other_assets: Vec<(String, String, String)> = Vec::new();
    for (unit, platform) in &members {
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
        let mut overlay_text = String::new();
        let (javascript, assets, sources) = match compile_unit(
            unit,
            *platform,
            false,
            matches!(platform, Platform::Browser),
            Some(&mut overlay_text),
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
        let mut assembled = vilan_core::const_eval::assemble_assets(&assets);
        let css = assembled
            .remove("css")
            .filter(|content| !content.is_empty());
        // Any non-css asset kind still lands on disk each round, exactly as
        // `write_assets` would put it (uniform with the build/run paths); it
        // just doesn't participate in classification — css is the only kind
        // the dev runtime knows how to hot-swap.
        for (kind, content) in assembled {
            other_assets.push((unit.name.clone(), kind, content));
        }
        next.push(hmr::LegArtifact {
            name: unit.name.clone(),
            is_browser: matches!(platform, Platform::Browser),
            bundle: javascript,
            css,
            sources: sources.into_iter().collect(),
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
    // and CSS sidecars are written verbatim.
    let dist = root.join("dist");
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
    for leg in &next {
        let bundle_path = dist.join(format!("{}.js", leg.name));
        let contents = if leg.is_browser {
            hmr::instrument(&leg.bundle, channel.port(), version, &leg.name)
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
    }
    for (name, kind, content) in &other_assets {
        let asset_path = dist.join(format!("{name}.{kind}"));
        if let Err(error) = fs::write(&asset_path, content) {
            eprintln!(
                "{} cannot write {}: {error}",
                paint::error_prefix(),
                asset_path.display()
            );
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
                // `dist/*.js`, exactly as `run_workspace` / `build_and_spawn_run`.
                let script = Path::new("dist").join(format!("{}.js", unit.name));
                match spawn_node(&script, args, Some(&root)) {
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
        Some(hmr::Push::Swap) => channel.push("swap", None),
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
fn manifest_fingerprint(root: &Path) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    fn collect_manifests(directory: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_manifests(&path, found);
            } else if path.file_name().and_then(|name| name.to_str()) == Some("vilan.toml") {
                found.push(path);
            }
        }
    }
    let mut manifests = Vec::new();
    collect_manifests(root, &mut manifests);
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
    env::temp_dir().join(format!("vilan-watch-{}.js", std::process::id()))
}

/// Removes the round's temp script, best effort. Called once the child that was
/// executing it is killed AND reaped: Windows has no unlink-while-open, so
/// rewriting the file under a live `node` is an intermittent sharing violation
/// — and leaving it behind is a temp-directory leak on every platform
/// (`windows-support.md` §5). A missing file is success.
fn remove_watch_script() {
    let script = watch_script_path();
    if let Err(error) = fs::remove_file(&script) {
        if error.kind() != std::io::ErrorKind::NotFound {
            eprintln!(
                "{} cannot remove {}: {error}",
                paint::warning_prefix(),
                script.display()
            );
        }
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
    if run_build_hooks(&project).is_err() {
        return None;
    }
    let launch = |script: &Path, cwd: Option<&Path>| match spawn_node(script, args, cwd) {
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
            let (javascript, assets, _sources) =
                compile_unit(&unit, Platform::default(), false, false, None).ok()?;
            // Assets go beside the *canonical* build output — `<entry>.css`, where
            // `build` writes them and the served program reads them — not beside the
            // /tmp watch script Node executes (which nothing serves). Each watch
            // round thus refreshes the on-disk sidecar for the dev loop (hmr.md §11
            // S0); the workspace arm below gets this for free via
            // `build_workspace_artifacts`.
            write_assets(&unit.entry.with_extension("js"), &assets);
            let script = watch_script_path();
            if let Err(error) = fs::write(&script, javascript) {
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
            if build_workspace_artifacts(&root, &members, false).is_err() {
                return None;
            }
            launch(
                &Path::new("dist").join(format!("{}.js", server.name)),
                Some(&root),
            )
        }
        Project::Library { name, .. } => {
            not_buildable_library(&name);
            None
        }
    }
}

/// Prints an `error: <message>` line and returns the failure code.
fn report_error(message: &str) -> ExitCode {
    eprintln!("{} {message}", paint::error_prefix());
    ExitCode::FAILURE
}

/// Reports that a `none`-platform package can't be built (it's a pure library).
fn no_host_platform() -> ExitCode {
    eprintln!(
        "{} the platform is `none` (a pure library); pick a host to build for with \
         `--platform node` or `--platform browser`",
        paint::error_prefix()
    );
    ExitCode::FAILURE
}

/// Reports that a `[library]` can't be built or run on its own — it's compiled only
/// as a dependency of an app.
fn not_buildable_library(name: &str) -> ExitCode {
    eprintln!(
        "{} `{name}` is a `[library]`, built only as a dependency of an app, not on its own. \
         Verify its platform contract with `vilan check`, or build an app that depends on it.",
        paint::error_prefix()
    );
    ExitCode::FAILURE
}

/// Checks a standalone `[library]`: it has no fixed build platform, so instead of a
/// single-platform compile it verifies the **platform contract** (§4.2) — every
/// module's `pkg::` imports must resolve for every platform that module's layer
/// serves. Reports any violation; clean ⇒ success.
fn check_library(dir: &Path, name: &str) -> ExitCode {
    let spec = vilan_core::manifest::resolve_library(dir);
    let violations = check_library_contract(&spec);
    if violations.is_empty() {
        println!(
            "{name}: {}",
            paint::out(paint::Style::GREEN, "platform contract OK")
        );
        ExitCode::SUCCESS
    } else {
        for violation in &violations {
            eprintln!("{} {}", paint::error_prefix(), violation.msg);
        }
        ExitCode::FAILURE
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
    if let Ok(relative) = entry.strip_prefix(pkg_root) {
        if !relative.as_os_str().is_empty() {
            return vilan_core::util::case_exact_mismatch(pkg_root, relative);
        }
    }
    let name = entry.file_name()?;
    vilan_core::util::case_exact_mismatch(&pkg_root_of(entry), Path::new(name))
}

/// Formats every `.vl` file under `paths` (a file, a directory walked
/// recursively, or the working directory when empty). In `--check` mode it only
/// reports files that would change; otherwise it rewrites them in place. The
/// formatter leaves a file untouched when it's already formatted or contains a
/// construct it can't yet print (it never produces non-round-tripping output).
fn fmt(paths: &[PathBuf], check: bool) -> ExitCode {
    let roots: Vec<PathBuf> = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths.to_vec()
    };
    let mut files = Vec::new();
    for root in &roots {
        collect_vl_files(root, &mut files);
    }
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

/// Collects every `.vl` file under `path` (recursing into directories), in a
/// stable (sorted) order.
fn collect_vl_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        if let Ok(entries) = fs::read_dir(path) {
            let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
            paths.sort();
            for entry in paths {
                collect_vl_files(&entry, out);
            }
        }
    } else if path.extension().and_then(|extension| extension.to_str()) == Some("vl") {
        out.push(path.to_path_buf());
    }
}

/// A buildable unit — a workspace member, a lone package, or a bare file: the
/// entry to compile, its package source root, the directory whose `vilan.toml`
/// declares its dependencies (for resolving the workspace), and its codegen
/// options. `name` labels a workspace member's `dist/<name>.js` output.
struct Unit {
    name: String,
    /// The entry file, resolved against the package root.
    entry: PathBuf,
    /// The package source root (where `import pkg::..` siblings resolve).
    pkg_root: PathBuf,
    /// The directory holding this unit's `vilan.toml` (from which its dependency
    /// workspace is resolved), or `None` for a bare file with no manifest.
    package_dir: Option<PathBuf>,
    options: BuildOptions,
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
    commands: Vec<String>,
}

impl BuildHooks {
    fn from_manifest(dir: &Path, manifest: &Manifest) -> BuildHooks {
        BuildHooks {
            dir: dir.to_path_buf(),
            commands: manifest.build_hooks().to_vec(),
        }
    }

    /// Runs every hook, in order, stopping at the first failure. Each command
    /// goes through the **platform shell** (`sh -c` / `cmd /C`) — hooks are
    /// shell one-liners with globs, pipes and `&&`, and an argv array would make
    /// the user hand-split them and lose all three. Streams are inherited, so a
    /// hook's output (and its TTY colors) reach the terminal as if run by hand;
    /// under `vilan build --stdout` that means a chatty hook shares the JS
    /// stream — redirect it in the command if that matters.
    fn run(&self) -> Result<(), String> {
        for command in &self.commands {
            eprintln!("{} {command}", paint::err(paint::Style::CYAN, "Running"));
            let spawned = shell_command(command).current_dir(&self.dir).spawn();
            let mut child = match spawned {
                // The Job object costs nothing here and buys the Windows
                // tree-kill: a hook that spawns a watcher of its own dies with
                // this process instead of outliving the session.
                Ok(child) => ManagedChild::adopt(child),
                Err(error) => {
                    return Err(format!(
                        "`[build] run` could not start `{command}`: {error}"
                    ));
                }
            };
            match child.wait() {
                Ok(status) if status.success() => {}
                Ok(status) => {
                    return Err(format!(
                        "`[build] run` command failed ({}): {command}",
                        status
                            .code()
                            .map(|code| format!("exit code {code}"))
                            .unwrap_or_else(|| "killed by a signal".to_string())
                    ));
                }
                Err(error) => {
                    return Err(format!("`[build] run` lost `{command}`: {error}"));
                }
            }
        }
        Ok(())
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
        /// default).
        platform: Option<Platform>,
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
}

/// Resolves the project from an optional path, then runs `action`. An explicit
/// file is a single entry; a directory (or no path, via the working directory)
/// is read from its `vilan.toml`.
fn with_project(path: Option<PathBuf>, action: impl FnOnce(Project) -> ExitCode) -> ExitCode {
    match resolve_project(path) {
        Ok(project) => action(project),
        Err(message) => {
            eprintln!("{} {message}", paint::error_prefix());
            ExitCode::FAILURE
        }
    }
}

fn resolve_project(path: Option<PathBuf>) -> Result<Project, String> {
    match path {
        // An explicit directory: the project rooted there.
        Some(path) if path.is_dir() => project_from_manifest(&path),
        // An explicit file (or a not-yet-existing path, so `compile` can report
        // the read error): a single entry, compiled directly with default options
        // (there's no manifest to read `[build]`/`target`/dependencies from).
        Some(path) => Ok(Project::Single {
            unit: Unit {
                name: String::new(),
                pkg_root: pkg_root_of(&path),
                entry: path,
                package_dir: None,
                options: BuildOptions::default(),
            },
            platform: None,
            // A bare file has no manifest, so it declares no hooks.
            hooks: BuildHooks::default(),
        }),
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
        options,
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
                    options,
                },
                entry.resolved_target().unwrap_or_default(),
            )
        })
        .collect();
    units.sort_by_key(|(_, platform)| !matches!(platform, Platform::Browser));
    units
}

/// Rejects two build units sharing a name — their `dist/<name>.js` outputs
/// would silently overwrite each other. (`none` members emit nothing, so they
/// can't collide.)
fn reject_output_collisions(members: &[(Unit, Platform)]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for (unit, platform) in members {
        if platform.is_none() {
            continue;
        }
        if !seen.insert(unit.name.as_str()) {
            return Err(format!(
                "two build units are both named `{}`, so their outputs would \
                 collide at dist/{}.js; rename one (the package name or the \
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
    // orchestration as a `[project]` — every entry builds to `dist/<name>.js`,
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

/// Resolves a unit's workspace and compiles its entry for `platform`, returning the
/// emitted JavaScript (or a failure code after reporting).
fn compile_unit(
    unit: &Unit,
    platform: Platform,
    emit_debug: bool,
    hmr: bool,
    overlay: Option<&mut String>,
) -> Result<(String, Vec<(String, String)>, Vec<(PathBuf, u64)>), ExitCode> {
    let workspace = match resolve_workspace(unit) {
        Ok(workspace) => workspace,
        Err(message) => {
            eprintln!("{} {message}", paint::error_prefix());
            return Err(ExitCode::FAILURE);
        }
    };
    // HMR instrumentation is opt-in per compile (an HMR-active `run --watch`,
    // browser legs only) — every other caller passes `false`, so `build`/`run`/
    // `check` output stays byte-identical.
    let mut options = unit.options;
    options.hmr = hmr;
    compile_to_js(
        &unit.entry,
        &unit.pkg_root,
        platform,
        &options,
        &workspace,
        emit_debug,
        overlay,
    )
}

/// Builds a lone package / bare file, writing `<entry>.js` (or printing to stdout).
fn build_single(unit: &Unit, stdout: bool, platform: Platform, emit_debug: bool) -> ExitCode {
    let (javascript, assets, _sources) = match compile_unit(unit, platform, emit_debug, false, None)
    {
        Ok(compiled) => compiled,
        Err(code) => return code,
    };
    if stdout {
        print!("{javascript}");
        return ExitCode::SUCCESS;
    }
    let output_path = unit.entry.with_extension("js");
    write_assets(&output_path, &assets);
    match fs::write(&output_path, javascript) {
        Ok(()) => {
            println!(
                "{} {} -> {}",
                paint::out(paint::Style::GREEN, "Compiled"),
                unit.entry.display(),
                paint::out(paint::Style::BOLD, &output_path.display().to_string())
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!(
                "{} cannot write {}: {error}",
                paint::error_prefix(),
                output_path.display()
            );
            ExitCode::FAILURE
        }
    }
}

/// Type-checks a lone package / bare file, writing no output.
fn check_single(unit: &Unit, platform: Platform, emit_debug: bool) -> ExitCode {
    match compile_unit(unit, platform, emit_debug, false, None) {
        Ok(_) => {
            println!(
                "{}: {}",
                unit.entry.display(),
                paint::out(paint::Style::GREEN, "no errors")
            );
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

/// Builds and runs a lone package's entry with Node, forwarding `args`.
fn run_single(unit: &Unit, args: &[String]) -> ExitCode {
    let (javascript, assets, _sources) =
        match compile_unit(unit, Platform::default(), false, false, None) {
            Ok(compiled) => compiled,
            Err(code) => return code,
        };
    // Const-eval assets (the CSS sidecar &c.) belong beside the *canonical* build
    // output — `<entry>.css`, where `build` writes them and a served page reads
    // them — not beside the temp script `run_node_script` hands Node, which the
    // program never reads. Same helper and placement as `build_single`, so `run`
    // keeps the on-disk sidecar fresh (const-eval.md §3; hmr.md §11 S0).
    write_assets(&unit.entry.with_extension("js"), &assets);
    run_node_script(&javascript, args)
}

/// Builds every host (non-`none`) member of a workspace into `<root>/dist/<name>.js`
/// — a `none` member is a pure library, compiled only as a dependency of a host.
/// Members build in declaration order (the client before the server, so the
/// server's `dist/client.js` exists). `--platform`/`--stdout` don't apply.
fn build_workspace(root: &Path, members: &[(Unit, Platform)], debug: bool) -> ExitCode {
    match build_workspace_artifacts(root, members, debug) {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

fn build_workspace_artifacts(
    root: &Path,
    members: &[(Unit, Platform)],
    debug: bool,
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
    for (unit, platform) in members {
        if platform.is_none() {
            continue;
        }
        let (javascript, assets, _sources) = compile_unit(unit, *platform, debug, false, None)?;
        let output = dist.join(format!("{}.js", unit.name));
        write_assets(&output, &assets);
        if let Err(error) = fs::write(&output, javascript) {
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
    }
    Ok(())
}

/// Type-checks every member of a workspace (each for its own platform; a `none`
/// library against the base layer).
fn check_workspace(members: &[(Unit, Platform)], debug: bool) -> ExitCode {
    let mut ok = true;
    for (unit, platform) in members {
        ok &= compile_unit(unit, *platform, debug, false, None).is_ok();
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
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
/// the project root (so it can read sibling `dist/*.js`). `args` are forwarded.
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
    if let Err(code) = build_workspace_artifacts(root, members, false) {
        return code;
    }
    // Run from the project root so the server reads sibling `dist/*.js`; the script
    // path is relative to that working directory.
    let status = spawn_node(
        &Path::new("dist").join(format!("{}.js", server.name)),
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
fn test(path: Option<PathBuf>) -> ExitCode {
    let tests = match discover_tests(path) {
        Ok(tests) => tests,
        Err(message) => {
            eprintln!("{} {message}", paint::error_prefix());
            return ExitCode::FAILURE;
        }
    };
    if tests.is_empty() {
        println!(
            "{}",
            paint::out(paint::Style::DIM, "no `*_test.vl` tests found")
        );
        return ExitCode::SUCCESS;
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
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
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
    let Some(directory) = file.parent().and_then(find_project_root) else {
        return Ok(bare());
    };
    let (manifest, _warnings) = read_manifest_quietly(&directory)?;
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
    let (javascript, _assets, _sources) = compile_to_js(
        file,
        &pkg_root,
        Platform::default(),
        &options,
        &workspace,
        false,
        None,
    )
    .map_err(|_| String::new())?;
    let script = env::temp_dir().join(format!("vilan-test-{}.js", std::process::id()));
    if let Err(error) = fs::write(&script, javascript) {
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
/// `$VILAN_STD`; the nearest ancestor of the entry file (then of the working
/// directory) containing `vilan/std/vilan.toml` — a checkout, so a `vilan`
/// built from this repo compiles against the working tree; else the binary's
/// own embedded std, materialized once to `~/.vilan/std-cache/<hash>/` — what
/// an installed binary uses, from any directory, with no checkout.
/// `resolve_std` reads the resulting package's `[library]` manifest (or, if
/// `$VILAN_STD` points at a bare source root with no manifest, uses it as the
/// base layer).
fn std_dir(entry: &Path) -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("VILAN_STD") {
        return Ok(PathBuf::from(path));
    }
    let starts = [
        entry
            .canonicalize()
            .ok()
            .and_then(|file| file.parent().map(Path::to_path_buf)),
        env::current_dir().ok(),
    ];
    for start in starts.iter().flatten() {
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
fn write_assets(output_js: &std::path::Path, assets: &[(String, String)]) {
    for (kind, content) in vilan_core::const_eval::assemble_assets(assets) {
        let path = output_js.with_extension(kind.as_str());
        if let Err(error) = fs::write(&path, content) {
            eprintln!(
                "{} cannot write {}: {error}",
                paint::error_prefix(),
                path.display()
            );
        } else {
            println!(
                "{}  {}",
                paint::out(paint::Style::GREEN, "Emitted"),
                paint::out(paint::Style::BOLD, &path.display().to_string())
            );
        }
    }
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
    options: &BuildOptions,
    workspace: &Workspace,
    emit_debug: bool,
    // When `Some`, an ANSI-free plain-text rendering of this file's diagnostics
    // is written here on a failed compile — the HMR error overlay's copy (hmr.md
    // §§2/§6). The terminal rendering below is untouched: this is a second,
    // additive pass over the SAME messages, never a redirect. Every other caller
    // passes `None` and pays nothing.
    overlay: Option<&mut String>,
) -> Result<(String, Vec<(String, String)>, Vec<(PathBuf, u64)>), ExitCode> {
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
    let std = match std_dir(file) {
        Ok(directory) => vilan_core::manifest::resolve_std(&directory),
        Err(error) => {
            eprintln!("{} {error}", paint::error_prefix());
            return Err(ExitCode::FAILURE);
        }
    };
    let mut output = None;

    // Fast path: a clean entry file reuses the shared content-addressed parse
    // cache (`vilan_core::parse_clean_cached`) — the same cache `std` and the
    // package modules use — so across `--watch` rounds an unchanged entry file
    // is served from the cache instead of re-parsed (backlog E12). A hit is
    // already lift-rewritten and `'static`; the handwritten frontend runs only
    // when the cache misses (a non-clean file), recovering a tree and naming its
    // diagnostics in a single fast-and-rich pass.
    let cached = vilan_core::parse_clean_cached(&src);

    // Analyzer and codegen diagnostics, collected as `(source, span, message)`
    // for ariadne — the source being the file the span indexes into, so each one
    // renders in its own file (backlog E16). Note-carrying ones render
    // separately (they still count against a clean build via `noted_errors`).
    let mut analyzer_errors: Vec<(SourceId, std::ops::Range<usize>, String)> = Vec::new();
    let mut noted_errors = 0usize;
    // The same diagnostics, captured as structured items for the HMR overlay
    // (only assembled into text when `overlay` is `Some`). Populated alongside
    // the terminal path — never in place of it — reusing each message verbatim.
    let mut overlay_diagnostics: Vec<hmr::OverlayDiagnostic> = Vec::new();

    // On a cache miss the handwritten frontend parses the entry, always returning
    // a (possibly recovered) tree alongside every diagnostic. A batch compile does
    // not analyze a file that failed to parse cleanly — its parse errors are
    // reported and the build fails — so the freshly parsed tree is taken only when
    // the parse produced no diagnostics.
    let mut parse_errors: Vec<vilan_core::parsing::ParseError> = Vec::new();
    let fresh_root: Option<vilan_core::Spanned<vilan_core::node::NodeList>> = match &cached {
        None => {
            let (tree, errors) = vilan_core::parsing::parse(src.as_str());
            let clean = errors.is_empty();
            parse_errors = errors;
            tree.filter(|_| clean).map(|(mut items, span)| {
                // Elements desugar, then bare-`?` marks become lift regions,
                // before analysis (element-syntax.md §4, expression-lifting.md);
                // the cached path gets both inside `parse_clean_cached`, so
                // each runs exactly once here.
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
            write_debug(file, "parse.out", &format!("{root:#?}"));
        }

        let mut program = analyze(root, source_ref, &std, pkg_root, file, platform, workspace);

        // Thread `std::context::Context` values as hidden parameters (a no-op
        // unless the program creates a context).
        context::thread_contexts(&mut program);

        // Infer which functions/closures are async (drives `async`/`await`
        // code generation).
        async_infer::infer(&mut program);
        // Reject an async `drop` body now that asyncness is settled
        // (destruction.md §5): teardown must be synchronous in v1. An awaiting
        // body is async only by inference, so this runs after `async_infer`.
        vilan_core::analyzer::check_async_drops(&mut program);
        // Teardown must be context-free (destruction.md §8): a `drop` body whose
        // call sites (scope exits) can thread no context is rejected. Runs after
        // `thread_contexts` fills `context_dependent_functions`.
        vilan_core::analyzer::check_context_drops(&mut program);
        vilan_core::platform_color::check(&mut program, platform);

        // Evaluate `const` expressions (proposal/const-eval.md); the results
        // serialize in place at transform time, the failures are ordinary
        // diagnostics.
        let (const_results, const_assets, const_errors) = vilan_core::const_eval::evaluate(
            &program,
            &vilan_core::options::BuildOptions::default(),
        );
        program.const_results = const_results;
        program.const_assets = const_assets;
        for (error, source) in const_errors {
            program.push_diagnostic(error, source);
        }

        // A dependency cycle among module-level initializers has no valid
        // declaration order (b33-emission-order.md §3), so it is an error
        // rather than a load-time `ReferenceError`. Runs last: the relation is
        // only meaningful for a program that analyzed cleanly.
        vilan_core::init_order::check_cycles(&mut program);

        for (index, error) in program.diagnostics.iter().enumerate() {
            let source = program.diagnostic_source(index);
            load_diagnostic_file(&mut diagnostic_files, &program, source);
            // Capture every diagnostic for the overlay (message + note verbatim),
            // located in the file its span indexes into (E16) — a module error
            // names the module, not the leg's entry; the terminal rendering
            // below is unchanged.
            let (overlay_name, overlay_text) = diagnostic_file(&diagnostic_files, source);
            overlay_diagnostics.push(hmr::OverlayDiagnostic::located(
                overlay_name,
                overlay_text,
                error.span.into_range(),
                error.msg.clone(),
                error.note.as_ref().map(|note| note.msg.clone()),
            ));
            // A note-carrying diagnostic renders directly (two labels — the
            // shared ariadne path has nowhere to put the secondary location);
            // plain ones keep the shared path.
            match &error.note {
                Some(note) => {
                    // A cross-source note reads its file so the sub-label can
                    // render in it (the trait's declaration in std, say). `None`,
                    // or the primary's own source, means the same file — which
                    // needs no second source.
                    let note_source = note.source.filter(|note_source| *note_source != source);
                    if let Some(note_source) = note_source {
                        load_diagnostic_file(&mut diagnostic_files, &program, note_source);
                    }
                    let (name, text) = diagnostic_file(&diagnostic_files, source);
                    let note_file = note_source
                        .map(|note_source| diagnostic_file(&diagnostic_files, note_source));
                    report_error_with_note(name, text, error, note_file);
                    noted_errors += 1;
                }
                None => analyzer_errors.push((source, error.span.into_range(), error.msg.clone())),
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
            let call_graph = CallGraph::build(&program);
            write_debug(file, "callgraph.out", &call_graph.debug_dump(&program));
        }

        if analyzer_errors.is_empty() && noted_errors == 0 {
            // `--print-chunks` (bundle-splitting.md S1): report what a split
            // build would chunk. Analysis-only — the emitted JavaScript below
            // is untouched — and gated on a clean analysis, so a failing build
            // reports its diagnostics, never a plan over a broken program.
            if PRINT_CHUNKS.load(std::sync::atomic::Ordering::Relaxed) {
                let chunk_plan = vilan_core::chunks::plan(&program);
                print!("{}", vilan_core::chunks::render(&chunk_plan, &filename));
            }
            match transform(&program, options) {
                // The leg's source set — each path paired with the content
                // hash it was COMPILED from — which the watch loop verifies
                // (by re-hashing, never by mtime) to skip a leg whose sources
                // didn't change (backlog E12, half b).
                Ok(javascript) => {
                    output = Some((
                        javascript,
                        program.const_assets.clone(),
                        program
                            .sources
                            .iter()
                            .cloned()
                            .zip(program.source_hashes.iter().copied())
                            .collect::<Vec<_>>(),
                    ))
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

    let clean = analyzer_errors.is_empty() && parse_errors.is_empty() && noted_errors == 0;
    // The overlay's copy of this leg's diagnostics (hmr.md §§2/§6): the analyzer/
    // codegen items captured above, plus the parse errors rendered with the SAME
    // `render` the terminal `report` uses — only the location prefix and framing
    // are added here. Assembled only when a caller asked for it and the build
    // failed.
    if let Some(sink) = overlay {
        if !clean {
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
            *sink =
                hmr::render_overlay(&filename, &overlay_diagnostics, hmr::OVERLAY_DIAGNOSTIC_CAP);
        }
    }
    // The entry's parse errors belong to the entry; the analyzer's carry their
    // own source.
    report(&diagnostic_files, analyzer_errors, parse_errors);

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
    let script = env::temp_dir().join(format!("vilan-run-{}.js", std::process::id()));
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
    let mut command = std::process::Command::new("node");
    command.arg(script).args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
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
/// (`windows-support.md` §6). Two things beyond the byte indexing:
///
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
        .with_index_type(ariadne::IndexType::Byte)
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

/// The (label, text) a diagnostic renders against. Every source a diagnostic
/// named has been loaded by then; the fallback keeps an unknown one visible
/// (message, no snippet) rather than rendering it against innocent text.
fn diagnostic_file(files: &HashMap<SourceId, (String, String)>, source: SourceId) -> (&str, &str) {
    files
        .get(&source)
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .unwrap_or(("<unknown>", ""))
}

/// The text a label may be drawn from: the file's own text when the span
/// actually indexes it at character boundaries, otherwise nothing.
///
/// ariadne slices the source by the label's byte range and **panics** on a
/// mid-codepoint index ("byte index N is not a char boundary"), which takes the
/// compiler thread down with it (backlog E16). Attribution is what makes spans
/// fit — this is the net under it: a span that does not index this text is a
/// bug, and the honest degrade is the message without a snippet, never a
/// clamped label pointing at innocent code.
fn snippet<'a>(text: &'a str, span: &std::ops::Range<usize>) -> &'a str {
    let indexes_this_text = span.start <= span.end
        && span.end <= text.len()
        && text.is_char_boundary(span.start)
        && text.is_char_boundary(span.end);
    if indexes_this_text { text } else { "" }
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
        Report::build(ReportKind::Error, (filename.to_string(), span.clone()))
            .with_config(diagnostic_config())
            .with_message(&message)
            .with_label(
                Label::new((filename.to_string(), span.clone()))
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

/// Renders one analyzer diagnostic that carries a secondary note
/// (diagnostics-standard.md C3): the primary label at the error's span, the
/// note as a second label at its own location ("first call here", "generated
/// by this attribute").
fn report_error_with_note(
    // The primary span's own file (name, contents) — a module's error renders
    // in the module (backlog E16).
    filename: &str,
    src: &str,
    error: &vilan_core::Error,
    // The note's own file when it lives elsewhere (name, contents) —
    // cross-source notes point into std or an imported module.
    note_file: Option<(&str, &str)>,
) {
    let Some(note) = &error.note else {
        return;
    };
    let primary_span = error.span.into_range();
    let note_span = note.span.into_range();
    let note_filename = note_file
        .map(|(name, _)| name.to_string())
        .unwrap_or_else(|| filename.to_string());
    let mut files = vec![(
        filename.to_string(),
        snippet(src, &primary_span).to_string(),
    )];
    match note_file {
        Some((name, text)) => files.push((name.to_string(), snippet(text, &note_span).to_string())),
        // Same file as the primary: the note's span must index the text the
        // primary already brought.
        None => {
            if snippet(src, &note_span).is_empty() {
                files[0].1 = String::new();
            }
        }
    }
    Report::build(
        ReportKind::Error,
        (filename.to_string(), primary_span.clone()),
    )
    .with_config(diagnostic_config())
    .with_message(error.msg.clone())
    .with_label(
        Label::new((filename.to_string(), primary_span))
            .with_message(error.msg.clone())
            .with_color(Color::Red),
    )
    .with_label(
        Label::new((note_filename, note_span))
            .with_message(note.msg.clone())
            .with_color(Color::Yellow),
    )
    .finish()
    // stderr, like the warnings (ratified call (f)).
    .eprint(sources(files))
    .unwrap();
}

/// Renders a single analyzer warning (e.g. an unused `[must_use]` result) — like
/// `report`, but `ReportKind::Warning` and non-fatal. Carries its own file too.
fn report_warning(filename: &str, src: &str, span: std::ops::Range<usize>, message: &str) {
    Report::build(ReportKind::Warning, (filename.to_string(), span.clone()))
        .with_config(diagnostic_config())
        .with_message(message)
        .with_label(
            Label::new((filename.to_string(), span.clone()))
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
                options: BuildOptions::default(),
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
        // Inverted — never produced, never sliced either.
        assert_eq!(snippet(multibyte, &(6..4)), "");
    }
}
