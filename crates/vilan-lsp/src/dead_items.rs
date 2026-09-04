//! E124's package clock: the union of the pruner's reachability across a
//! package's entries, computed OUT OF BAND and served to the paint
//! (`proposal/dead-code-paint.md` §2.4, determination 7).
//!
//! **Why it cannot ride the analysis in hand.** The language server analyzes
//! the OPEN file as the entry — `Program::sources[0]` is the entry file — so
//! when the user edits `src/store.vl` the program in hand is rooted at
//! `store.vl` and has no `main`. Measured on kolt: nine of twelve hand-written
//! files report NO-MAIN, and the reachability walk has no root to start from
//! (§2.1, probe P5). Every term of the union, *including* the term for the
//! entry the open file belongs to, has to come from a separately computed
//! per-entry set.
//!
//! **Why it cannot ride M21's base cache.** The cache stores the pre-entry
//! world and revalidates by content: a buffered edit to a sibling module evicts
//! the world of every entry that loads it, and the common editing case — typing
//! in a module the entry loads — is exactly the case the cache cannot serve
//! (§2.3). So the union's inputs are one full analysis per entry, 0.4–1.2 s
//! each cold, and they are paid on this clock rather than on the debounced
//! diagnostics path. The walk over them is cheap by comparison — 8.2 ms for
//! kolt's three entries — and the debounced path pays neither: it reads a
//! finished set and does a hash lookup per candidate.
//!
//! **The keys are `(canonical path, name span)`, never entity ids.** Ids are
//! minted per analysis, so the same declaration has three different ids across
//! three entries' programs and the union would be meaningless on them.

use std::path::{Path, PathBuf};

use vilan_core::Manifest;
use vilan_core::cancel::CancelToken;
use vilan_core::dead_items::{ItemKey, reached_item_keys};
use vilan_core::fx::FxHashSet as HashSet;

use crate::document::Document;

/// One package's union, as of the moment it was computed.
pub struct PackageReach {
    /// The union of every entry's reachability, over the whole program each
    /// entry loads — std and dependencies included, which costs nothing to
    /// carry and saves the consumer a filter it would have to get right.
    pub reached: HashSet<ItemKey>,
    /// The package edit revision this union was computed against. The paint is
    /// served only while it still matches the package's current revision, which
    /// is what makes withdrawal instant and restoration the clock's job.
    pub revision: u64,
}

/// The package's declared entries as `(name, path)`, or `None` when the
/// manifest declares no package to have entries — a `[library]` (validation
/// refuses entries on one outright) or a `[project]` workspace root.
///
/// Both manifest shapes answer: the classic single-entry form
/// (`[package] entry = "main.vl"`, named by its own path) and the
/// `[entry.<name>]` form kolt uses (client / server / probe).
pub fn entry_paths(manifest_dir: &Path) -> Option<Vec<(String, PathBuf)>> {
    let contents = std::fs::read_to_string(manifest_dir.join("vilan.toml")).ok()?;
    let (manifest, _warnings) = Manifest::parse(&contents).ok()?;
    entries_of(manifest_dir, &manifest)
}

fn entries_of(manifest_dir: &Path, manifest: &Manifest) -> Option<Vec<(String, PathBuf)>> {
    let package = manifest.package.as_ref()?;
    let pkg_root = manifest_dir.join(package.root());
    if manifest.entries.is_empty() {
        return Some(vec![(
            package.entry().display().to_string(),
            pkg_root.join(package.entry()),
        )]);
    }
    Some(
        manifest
            .entries
            .iter()
            .map(|(name, entry)| (name.clone(), pkg_root.join(entry.path(name))))
            .collect(),
    )
}

/// Compute the union: one analysis per entry, then one paint walk per entry,
/// then the union of their keys.
///
/// `entry_text` supplies each entry's current text — the server passes the open
/// buffer when it has one, so the union describes what the user is looking at
/// rather than what is on disk, and falls back to the file otherwise.
///
/// `None` — no paint at all — in three cases, and each is the safe direction:
///
/// - an entry whose text cannot be read, or whose analysis produces no program;
/// - an entry with no `main`, so the walk has no root and the union would be a
///   guess;
/// - **any diagnostic anywhere in the package.** A broken parse suppresses the
///   package's grays wholesale, not merely its own file's (§3.3): a salvaged
///   parse can lose a whole block or the file's entire tail, and a smaller
///   program reads to a reachability walk as a deader one. An entry's analysis
///   loads every module that entry builds, so one broken module is one refused
///   union.
///
/// `cancel` stops a union an edit has already invalidated. A package's union
/// costs one full analysis per entry, so a burst that would otherwise start a
/// second one while the first is still walking `client.vl` is exactly what this
/// is for — the same instrument M26 gave the per-document analyses, for the
/// same reason.
///
/// Called from `spawn_blocking`; it does a full analysis per entry and must
/// never run on a request path.
pub fn compute(
    entries: &[(String, PathBuf)],
    std_dir: &Path,
    revision: u64,
    cancel: &CancelToken,
    entry_text: impl Fn(&Path) -> Option<String>,
) -> Option<PackageReach> {
    let mut reached: HashSet<ItemKey> = HashSet::default();
    for (_, entry) in entries {
        let text = entry_text(entry)?;
        let document = Document::analyze_cancellable(&text, std_dir, entry, cancel)?;
        if !document.diagnostics.is_empty() {
            return None;
        }
        let program = document.program.as_ref()?;
        reached.extend(reached_item_keys(program)?);
    }
    Some(PackageReach { reached, revision })
}

/// E124's cost, on a GENERATED multi-entry exhibit (`dead-code-paint.md` §6.3).
///
/// The subject is three entries over a lucide-sized module (1,791 mechanical
/// functions) and a shared `theme`-shaped module whose accessors are reached
/// only through `const` initializers — kolt's SHAPE, sized like kolt, with
/// nothing copied from anyone's checkout.
///
/// What is measured is the part that could land on a request path if it were
/// built wrong: the union WALK over already-analyzed programs. The paper's
/// figure is 8.2 ms best / 13.3 ms typical for kolt's three entries in a
/// RELEASE build; a debug suite is an order of magnitude slower and the bound
/// below is set for a debug build under load, so it is a mechanism bound (no
/// per-item scan of a 1,791-function program gets under it) rather than a
/// tuning one. The union's real cost — one full analysis per entry — is not
/// asserted here at all: it is paid on the package clock, off every request
/// path, which is the whole design.
#[cfg(test)]
mod cost {
    use super::*;
    use crate::document::Document;
    use crate::document::tests::std_root;
    use crate::keystroke::gate::{exhibit_module, loadavg_1m, profile, thread_cpu_now};
    use std::time::Instant;

    const MANIFEST: &str = "[package]\nname = \"exhibit\"\ndefault-entry = \"server\"\n\n\
         [entry.client]\n\n[entry.server]\n\n[entry.probe]\n";

    /// `theme.vl`'s shape, which is the one the const-edge exemption exists
    /// for: every accessor is called ONLY from a `const` module binding's
    /// initializer, so the emission graph has no edge to any of them.
    const THEME: &str = "struct Theme { ink: i32, paper: i32 }\n\n\
         fun paint_ink1(): i32 {\n\t17\n}\n\n\
         fun paint_ink2(): i32 {\n\t29\n}\n\n\
         fun paint_paper(): i32 {\n\t41\n}\n\n\
         fun theme_new(): Theme {\n\tTheme { ink = paint_ink1() + paint_ink2(), paper = paint_paper() }\n}\n\n\
         let default_theme: Theme = const theme_new();\n\n\
         fun theme_ink(): i32 {\n\tdefault_theme.ink\n}\n\n\
         fun theme_paper(): i32 {\n\tdefault_theme.paper\n}\n\n\
         fun never_called_by_anyone(): i32 {\n\t0\n}\n";
    /// How much of the generated module the `client` leg reaches. kolt's own
    /// client leg reaches 624 ids, and the walk's cost is proportional to the
    /// nodes it VISITS rather than to the program it visits them in — so an
    /// entry that names four icons out of 1,791 would produce a walk that
    /// measures almost nothing, and a bound set on it would hold whatever the
    /// walk did.
    const CLIENT_REACHES: usize = 600;

    /// The `client` leg: a fixed preamble plus `CLIENT_REACHES` calls into the
    /// generated module, so the walk visits a kolt-sized share of the graph.
    fn client_module() -> String {
        let names: Vec<String> = (0..CLIENT_REACHES)
            .map(|index| format!("icon_{index:04}"))
            .collect();
        // Built piece by piece rather than as one continued literal: `cargo
        // fmt` rewraps a `\`-continued format string and would inject its own
        // indentation into the generated source.
        let mut text = String::new();
        text.push_str("import pkg::table::{ ");
        text.push_str(&names.join(", "));
        text.push_str(" };\n");
        text.push_str("import pkg::theme::theme_ink;\n\n");
        text.push_str("fun panel(): i32 {\n\tmut total = theme_ink();\n");
        for name in &names {
            text.push_str(&format!("\ttotal = total + {name}();\n"));
        }
        text.push_str("\ttotal\n}\n\n");
        text.push_str("fun main() {\n\tlet total = panel();\n}\n");
        text
    }

    const SERVER: &str = "import pkg::theme::theme_paper;\n\n\
         fun serve(): i32 {\n\ttheme_paper()\n}\n\n\
         fun main() {\n\tlet answer = serve();\n}\n";
    const PROBE: &str = "import pkg::theme::theme_ink;\n\n\
         fun main() {\n\tlet ink = theme_ink();\n}\n";

    fn exhibit() -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "vilan-e124-cost-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("src")).expect("a scratch directory");
        std::fs::write(directory.join("vilan.toml"), MANIFEST).expect("the manifest");
        std::fs::write(
            directory.join("src/table.vl"),
            exhibit_module(crate::keystroke::gate::GATE_FUNCTIONS),
        )
        .expect("the generated module");
        std::fs::write(directory.join("src/client.vl"), client_module()).expect("the client leg");
        for (relative, contents) in [
            ("src/theme.vl", THEME),
            ("src/server.vl", SERVER),
            ("src/probe.vl", PROBE),
        ] {
            std::fs::write(directory.join(relative), contents).expect("a source file");
        }
        directory
    }

    #[test]
    #[ignore = "E124: the union-walk cost pin — a generated 1,791-function three-entry exhibit, minutes of analysis; run deliberately (proposal/dead-code-paint.md §6.3)"]
    fn the_union_walk_costs_what_the_paper_says_it_does() {
        let directory = exhibit();
        let entries = entry_paths(&directory).expect("three declared entries");
        assert_eq!(entries.len(), 3, "the exhibit declares three entries");
        // The union's INPUTS: one full analysis per entry, off the request
        // path. Recorded, never asserted — the clock is where they are paid.
        let mut programs = Vec::new();
        for (name, path) in &entries {
            let text = std::fs::read_to_string(path).expect("the entry file");
            let started = Instant::now();
            let document = Document::analyze(&text, &std_root(), path);
            let analysis = started.elapsed().as_secs_f64() * 1000.0;
            assert!(
                document.diagnostics.is_empty(),
                "{name} analyzes cleanly: {:?}",
                document
                    .diagnostics
                    .iter()
                    .map(|e| &e.msg)
                    .collect::<Vec<_>>(),
            );
            println!(
                "E124 {{\"section\":\"union_input\",\"entry\":\"{name}\",\"profile\":\"{}\",\
                 \"load\":\"{}\",\"analysis_ms\":{analysis:.1}}}",
                profile(),
                loadavg_1m(),
            );
            programs.push(document);
        }
        // The WALK, amortized over repetitions, on the calling thread's own CPU
        // clock (M15) so a loaded box cannot make the number mean something
        // else.
        const REPETITIONS: usize = 5;
        let cpu_started = thread_cpu_now();
        let wall_started = Instant::now();
        let mut items = 0;
        for _ in 0..REPETITIONS {
            let mut reached = vilan_core::fx::FxHashSet::default();
            for document in &programs {
                let program = document.program.as_ref().expect("an analyzed program");
                reached.extend(
                    vilan_core::dead_items::reached_item_keys(program)
                        .expect("each entry has a `main`"),
                );
            }
            items = reached.len();
        }
        let wall = wall_started.elapsed().as_secs_f64() * 1000.0 / REPETITIONS as f64;
        let cpu = cpu_started.zip(thread_cpu_now()).map(|(before, after)| {
            after.saturating_sub(before).as_secs_f64() * 1000.0 / REPETITIONS as f64
        });
        println!(
            "E124 {{\"section\":\"union_walk\",\"subject\":\"syn1791x3\",\"profile\":\"{}\",\
             \"load\":\"{}\",\"reps\":{REPETITIONS},\"cpu_ms\":{},\"wall_ms\":{wall:.1},\
             \"reached\":{items}}}",
            profile(),
            loadavg_1m(),
            cpu.map_or_else(|| "null".to_string(), |value| format!("{value:.1}")),
        );
        // The whole union must actually be doing the work, or the number below
        // is vacuous: the three entries together reach the four named icons,
        // the theme accessors behind the `const` initializer, and `main`.
        assert!(
            items > CLIENT_REACHES,
            "the union reached only {items} items — the exhibit is not exercising the \
             walk, whose cost is proportional to the nodes it VISITS",
        );
        let _ = std::fs::remove_dir_all(&directory);
        let Some(cpu) = cpu else {
            panic!(
                "no thread CPU clock on this host, so the pin cannot assert anything load-proof (M15)"
            );
        };
        assert!(
            cpu < UNION_WALK_BUDGET_MS,
            "the three-entry union walk took {cpu:.1} ms of CPU on the 1,791-function exhibit, \
             over the {UNION_WALK_BUDGET_MS:.0} ms budget (loadavg {}, {wall:.1} ms of wall, {} \
             profile). The walk is not on the debounced path — it rides the package clock — but a \
             walk that grows out of this bound is a walk that has stopped being proportional to \
             the graph",
            loadavg_1m(),
            profile(),
        );
    }

    /// The budget for the three-entry union walk, and it is a MECHANISM bound
    /// rather than a tuning one.
    ///
    /// Measured on this exhibit — 613 items reached across the three legs,
    /// against kolt's own client leg at 624 — in a **debug** build at
    /// **loadavg 117**: **10.6 ms of thread CPU**, 70.2 ms of wall. The
    /// paper's release figure for kolt's three entries is 8.2 ms best,
    /// 13.3 ms typical, so the shape agrees. The bound is fourteen times the
    /// measurement, which is room for a slower host and none at all for a
    /// walk that has stopped being proportional to the graph: anything that
    /// rescanned the 1,791-function program per candidate would be seconds,
    /// not tens of milliseconds.
    ///
    /// Recorded beside it, and deliberately NOT asserted: the union's inputs,
    /// one full analysis per entry — 9,187 ms / 1,572 ms / 1,282 ms for
    /// client / server / probe in the same debug build under the same load.
    /// Those are the real cost, they are why the union rides a package clock
    /// instead of the diagnostics path, and a budget on them would be a
    /// budget on the analyzer wearing E124's name.
    const UNION_WALK_BUDGET_MS: f64 = 150.0;
}
