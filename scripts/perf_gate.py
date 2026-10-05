#!/usr/bin/env python3
"""perf_gate.py — the performance gates (M105, `proposals/projects/vilan/proposal/performance-gates.md`, RULED Q1–Q12).

    scripts/perf_gate.py gate      --vilan target/release/vilan [--class ci] [--counter auto] [--json OUT]
    scripts/perf_gate.py measure   --vilan BIN [--json OUT]                     # the table, no verdict
    scripts/perf_gate.py ratchet   --from MEASURED.json [--release]             # the seal's ceiling ratchet
    scripts/perf_gate.py seal      --vilan TIP --base RELEASE --kolt DIR [--tip-kolt MIGRATED] [--lsp-json BASE.json TIP.json]
    scripts/perf_gate.py calibrate --vilan BIN --kolt DIR                       # S4: genapp vs kolt phase split
    scripts/perf_gate.py report    --verdict perf-<sha>.json --out perf/report-vX.Y.Z.md

The three tiers (§3):

- T1, counters and shapes, live in nextest (`VILAN_COUNTERS`, the growth pins) and are not this script's.
- T2, instruction budgets (`gate`): each row of `perf/budgets.toml` is a subject (an example, the seeded
  kolt-shaped app of `perf_genapp.py`, a plain package of N modules) whose `vilan check` costs a COUNT of
  user-space instructions (`instructions:u`, or callgrind Ir where the machine has no PMU — a row names
  its counter). A row is red past `ceiling x (1 + tolerance) x its bumps`, compared CUMULATIVELY against
  the ceiling, never against the parent commit (§4). A ceiling is enforced only on the machine CLASS it
  was measured on: counts are per ISA and per libc, so a ceiling taken on the reference machine says
  nothing about a CI runner (§2.4, §6.3). A class with no ceiling for a row reports the count and does
  not refuse — that is how a new class (CI, before S2's spread measurement) is adopted: its first
  measured JSON goes through `ratchet --from` at the seal. The GROWTH rows hold everywhere, because they
  are ratios within one run: a package twice the size may cost at most `max_ratio` times as much
  (≤ x2.3 per doubling, Q11; M107).
- T3, the seal (`seal`): the tip against the previous release on kolt — CPU time and peak RSS by
  interleaved runs, refused above loadavg 2 (Q3), instructions beside them at any load — plus the T2 gate
  and E121's rows (Q9: red-reporting until green at two consecutive seals, then blocking). It writes
  `perf-<sha>.json`, which `scripts/cut-release.sh` reads (S6): no green verdict at the commit to be
  tagged, no cut, unless `--allow-perf-regression "<reason>"`.

Across a BREAKING release the one kolt source cannot serve both sides: the release's source no longer checks
under the tip, and a migrated one does not check under the release. `seal --tip-kolt DIR` gives the tip its
own source — a prepared, already-migrated tree used as-is (copied to the scratch location, no git needed in
it) — while the base keeps `--kolt`/`--commit`. Before anything is measured, each side must check its own
source with no errors, or the seal is refused; the verdict and its report say the two sides checked
different sources, and why.

A clock appears only in T3, which runs on the owner's quiet machine and refuses a CPU verdict under load;
every other figure is a count, the same on any machine of a class. Linux only (perf_event_open, wait4)."""
import argparse
import datetime
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import perf_count  # noqa: E402
import perf_genapp  # noqa: E402

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    tomllib = None

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BUDGETS = os.path.join(REPO, "perf", "budgets.toml")
DEFAULT_VERDICTS = os.path.join(os.path.expanduser("~"), ".vilan", "perf-verdicts")


# ---------------------------------------------------------------------------- budgets.toml


def load_budgets(path=BUDGETS):
    if tomllib is None:
        sys.exit("perf_gate: Python 3.11+ is needed to read perf/budgets.toml (tomllib)")
    with open(path, "rb") as handle:
        data = tomllib.load(handle)
    data.setdefault("row", [])
    data.setdefault("bump", [])
    data.setdefault("growth", [])
    data.setdefault("e121", [])
    return data


def toml_value(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return f"{value:_}" if abs(value) >= 10_000 else str(value)
    if isinstance(value, float):
        return repr(value)
    return json.dumps(value)


def write_budgets(data, path=BUDGETS):
    """The file's own shape, written back: the header comment, then each table array in turn."""
    with open(path) as handle:
        header = []
        for line in handle:
            if line.startswith("[["):
                break
            header.append(line)
    out = ["".join(header).rstrip() + "\n"]
    for key in ("row", "growth", "bump", "e121"):
        for table in data.get(key, []):
            out.append(f"\n[[{key}]]\n")
            for name, value in table.items():
                out.append(f"{name} = {toml_value(value)}\n")
    with open(path, "w") as handle:
        handle.write("".join(out))


def effective_ceiling(data, row):
    ceiling = row["ceiling"] * (1 + data.get("tolerance", 0.01))
    for bump in data["bump"]:
        if bump["subject"] == row["subject"] and bump.get("class", row["class"]) == row["class"]:
            ceiling *= bump["ratio"]
    return int(ceiling)


# ---------------------------------------------------------------------------- subjects


def subject_dir(subject, work):
    """The package a subject names, materialized under `work` at a FIXED path (so the macro-expansion
    table `vilan check` keeps per package path is reused, not re-minted per run — N137)."""
    kind, _, argument = subject.partition(":")
    if kind == "example":
        return os.path.join(REPO, "vilan", "examples", argument)
    target = os.path.join(work, subject.replace(":", "-"))
    if kind == "genapp":
        seed = int(argument or 46)
        stamp = os.path.join(target, ".perf-genapp")
        if not os.path.exists(stamp):
            shutil.rmtree(target, ignore_errors=True)
            perf_genapp.generate(target, seed=seed)
            open(stamp, "w").close()
        return target
    if kind == "plain":
        stamp = os.path.join(target, ".perf-plain")
        if not os.path.exists(stamp):
            shutil.rmtree(target, ignore_errors=True)
            perf_genapp.generate_plain(target, int(argument))
            open(stamp, "w").close()
        return target
    sys.exit(f"perf_gate: unknown subject kind {kind!r} in {subject!r}")


def measure_subject(vilan, subject, work, counter, env=None):
    """One subject's `vilan check`: a discarded warm-up (std materialized, macro table written), then the
    measured run."""
    directory = subject_dir(subject, work)
    # The members of a multi-entry package one after another (M100 overlaps them by default): two
    # threads racing for std's clean-parse cache make the count move by ~0.2% run to run, which is
    # noise a ceiling at +1% should not have to absorb. The work is the same work either way.
    # And the macro-expansion tables under the work directory (N137's `VILAN_CHECK_CACHE`): the gate
    # warms its own subjects and leaves the developer's `~/.vilan/check-cache` alone.
    env = dict(os.environ if env is None else env, VILAN_SEQUENTIAL_CHECK="1",
               VILAN_CHECK_CACHE=os.path.join(os.path.abspath(work), "check-cache"))
    perf_count.measure([vilan, "check", "."], cwd=directory, env=env, counter=counter)
    result = perf_count.measure([vilan, "check", "."], cwd=directory, env=env, counter=counter,
                                log=os.path.join(work, subject.replace(":", "-") + ".log"))
    result["subject"] = subject
    return result


def resolve_counter(counter):
    if counter == "auto":
        return "instructions:u" if perf_count.hardware_counter_available() else "callgrind"
    return counter


def all_subjects(data):
    subjects = list(data.get("subjects", [])) + [row["subject"] for row in data["row"]]
    for growth in data["growth"]:
        subjects += [f"plain:{growth['small']}", f"plain:{growth['large']}"]
    seen = []
    for subject in subjects:
        if subject not in seen:
            seen.append(subject)
    return seen


def measure_all(vilan, data, work, counter, only=None):
    counter = resolve_counter(counter)
    results = {}
    for subject in all_subjects(data):
        if only and subject not in only:
            continue
        results[subject] = measure_subject(vilan, subject, work, counter)
        r = results[subject]
        print(f"  {subject:24} {r['instructions']:>16,}  {counter}  exit {r['exit']}", flush=True)
    return counter, results


# ---------------------------------------------------------------------------- T2


def judge(data, results, counter, klass):
    """Rows and growth checks against the measurement. Returns (lines, red)."""
    lines, red = [], []
    for row in data["row"]:
        result = results.get(row["subject"])
        if result is None:
            continue
        if result["exit"] != 0:
            red.append(f"{row['subject']}: `vilan check` exited {result['exit']}")
            continue
        if row["counter"] != counter or row["class"] != klass:
            continue
        limit = effective_ceiling(data, row)
        ratio = result["instructions"] / row["ceiling"]
        state = "ok" if result["instructions"] <= limit else "RED"
        lines.append(f"  {state:4} {row['subject']:24} {result['instructions']:>16,} / ceiling "
                     f"{row['ceiling']:>16,} (x{ratio:.4f}, limit x{limit / row['ceiling']:.4f})")
        if state == "RED":
            red.append(f"{row['subject']} x{ratio:.4f} of its ceiling ({counter}, class {klass})")
    unceiled = [s for s in results if s.split(':')[0] != 'plain' and not any(
        r["subject"] == s and r["counter"] == counter and r["class"] == klass for r in data["row"])]
    for subject in unceiled:
        lines.append(f"  --   {subject:24} {results[subject]['instructions']:>16,}  (no ceiling for class "
                     f"{klass!r} / {counter}: reported, not enforced — adopt it at the seal with "
                     f"`ratchet --from`)")
    for growth in data["growth"]:
        small = results.get(f"plain:{growth['small']}")
        large = results.get(f"plain:{growth['large']}")
        if not small or not large:
            continue
        ratio = large["instructions"] / small["instructions"]
        state = "ok" if ratio <= growth["max_ratio"] else "RED"
        lines.append(f"  {state:4} growth plain {growth['small']} -> {growth['large']} modules: x{ratio:.3f} "
                     f"(at most x{growth['max_ratio']}, Q11/M107)")
        if state == "RED":
            red.append(f"growth x{ratio:.3f} > x{growth['max_ratio']} per doubling")
    return lines, red


def command_measure(options):
    data = load_budgets(options.budgets)
    counter, results = measure_all(options.vilan, data, options.work, options.counter, options.subject)
    if options.json:
        dump_measurement(options.json, options.vilan, counter, options.klass, results)


def dump_measurement(path, vilan, counter, klass, results):
    version = subprocess.run([vilan, "--version"], capture_output=True, text=True).stdout.strip()
    with open(path, "w") as handle:
        json.dump({"vilan": version, "counter": counter, "class": klass, "machine": machine(),
                   "results": results}, handle, indent=1)


def machine():
    model = ""
    try:
        with open("/proc/cpuinfo") as handle:
            model = next((line.split(":", 1)[1].strip() for line in handle if line.startswith("model name")), "")
    except OSError:
        pass
    return {"cpu": model, "libc": " ".join(platform.libc_ver()), "kernel": platform.release()}


def command_gate(options):
    data = load_budgets(options.budgets)
    counter, results = measure_all(options.vilan, data, options.work, options.counter)
    lines, red = judge(data, results, counter, options.klass)
    print("\n".join(lines))
    if options.json:
        dump_measurement(options.json, options.vilan, counter, options.klass, results)
    if red:
        print("T2 VERDICT: RED — " + "; ".join(red))
        print("  A change that legitimately costs more adds a [[bump]] row to perf/budgets.toml in the same "
              "commit (subject, ratio, reason, item); the owner approves one over 3% at the seal (Q6).")
        return 1
    print("T2 VERDICT: green")
    return 0


# ---------------------------------------------------------------------------- the seal's ratchet


def command_ratchet(options):
    """Set or lower ceilings from a measured JSON (Q5): a row with no ceiling for the measurement's class
    and counter is adopted at measured x 1 (the tolerance rides on top at judgement); a row more than 2%
    under its ceiling is lowered to the new count. `--release` resets the bumps (they live one release)."""
    data = load_budgets(options.budgets)
    with open(options.source) as handle:
        measured = json.load(handle)
    counter, klass = measured["counter"], measured["class"]
    stamp = options.stamp or measured["vilan"]
    changes = []
    for subject, result in measured["results"].items():
        if subject.startswith("plain:") or result["exit"] != 0:
            continue
        row = next((r for r in data["row"] if r["subject"] == subject and r["counter"] == counter
                    and r["class"] == klass), None)
        if row is None:
            data["row"].append({"subject": subject, "counter": counter, "class": klass,
                                "ceiling": result["instructions"], "measured_at": stamp})
            changes.append(f"adopted {subject} ({klass}/{counter}) at {result['instructions']:,}")
        elif result["instructions"] < row["ceiling"] * 0.98:
            changes.append(f"lowered {subject} ({klass}/{counter}) {row['ceiling']:,} -> {result['instructions']:,}")
            row["ceiling"] = result["instructions"]
            row["measured_at"] = stamp
    if options.release:
        if data["bump"]:
            changes.append(f"reset {len(data['bump'])} bump row(s) at the release")
        data["bump"] = []
    if options.dry_run:
        print("\n".join(changes) or "nothing to ratchet")
        return 0
    write_budgets(data, options.budgets)
    print("\n".join(changes) or "nothing to ratchet")
    return 0


# ---------------------------------------------------------------------------- T3 + E121: the seal


def prepare_kolt(kolt, commit, scratch):
    copy = os.path.join(scratch, "kolt")
    os.makedirs(copy)
    archive = subprocess.run(["git", "-C", kolt, "archive", commit], stdout=subprocess.PIPE, check=True)
    subprocess.run(["tar", "-x", "-C", copy], input=archive.stdout, check=True)
    for extra in ("search-dict", "src/search-dict"):  # untracked inputs the const pass reads
        source = os.path.join(kolt, extra)
        if os.path.isdir(source) and not os.path.exists(os.path.join(copy, extra)):
            shutil.copytree(source, os.path.join(copy, extra))
    sha = subprocess.run(["git", "-C", kolt, "rev-parse", "--short=8", commit],
                         capture_output=True, text=True).stdout.strip()
    return copy, sha


def prepare_tree(tree, scratch):
    """A prepared source (`--tip-kolt`): the directory as it stands — no git is run in it — copied to the
    scratch location beside the base's copy, minus build output, databases and nested worktrees."""
    copy = os.path.join(scratch, "kolt-tip")
    shutil.copytree(tree, copy, ignore=shutil.ignore_patterns("dist", "worktrees", "node_modules", ".git", "*.db",
                                                              "target"))
    return copy


def side_envs(options):
    """The base's and the tip's environments: each side's std (`--base-std`, `--tip-std`)."""
    env_base, env_tip = dict(os.environ), dict(os.environ)
    if options.tip_std:
        env_tip["VILAN_STD"] = options.tip_std
    if options.base_std:
        env_base["VILAN_STD"] = options.base_std
    return env_base, env_tip


def prepare_sources(options, scratch):
    """The two sides' kolt copies: one copy of `--kolt` at `--commit` for both, or — across a breaking
    release — that copy for the base and the prepared `--tip-kolt` tree for the tip."""
    base_copy, kolt_sha = prepare_kolt(options.kolt, options.commit, scratch)
    sources = {"base": {"copy": base_copy, "label": f"kolt@{kolt_sha}"},
               "tip": {"copy": base_copy, "label": f"kolt@{kolt_sha}"}, "kolt": kolt_sha, "different": False}
    if options.tip_kolt:
        sources["tip"] = {"copy": prepare_tree(options.tip_kolt, scratch),
                          "label": f"the prepared tree {os.path.abspath(options.tip_kolt)}"}
        sources["different"] = True
    if not options.base_std:
        for side in ("base", "tip"):
            if inside_a_checkout(sources[side]["copy"]):
                # A binary run inside a vilan checkout reads THAT checkout's std, not its embedded one, so the
                # release would be measured with the tip's std — a different program.
                sys.exit(f"perf_gate: the kolt copy {sources[side]['copy']} sits inside a vilan checkout; pass "
                         "--base-std (the release's std) or a --scratch outside it")
    return sources


def different_sources_note(sources):
    """What the verdict says when the two sides checked different sources (`--tip-kolt`)."""
    return (f"the two sides checked DIFFERENT sources: the base {sources['base']['label']}, the tip "
            f"{sources['tip']['label']} — a breaking release, whose base source does not check under the tip "
            "(nor the migrated tree under the base). Each side's check was clean; the ratios compare the same "
            "app before and after its migration, not one text.")


def refuse_unchecked_sources(options, sources):
    """Across a breaking release each side checks its OWN source, so a ratio means something only when both
    checks are clean: run each once, before anything is measured, and answer the refusals (empty: both
    clean). A same-source seal keeps its old rule (a disagreement is noted, not refused)."""
    if not sources["different"]:
        return []
    env_base, env_tip = side_envs(options)
    refusals = []
    for side, binary, env in (("base", options.base, env_base), ("tip", options.tip, env_tip)):
        binary = os.path.abspath(binary) if os.sep in binary else binary
        run = subprocess.run([binary, "check", "."], cwd=sources[side]["copy"], env=env, capture_output=True,
                             text=True)
        if run.returncode != 0:
            first = next((line for line in (run.stderr + run.stdout).splitlines() if line.strip()), "")
            refusals.append(f"the {side}'s source ({sources[side]['label']}) does not check under the {side} "
                            f"compiler (exit {run.returncode}): {first}")
    return refusals


def inside_a_checkout(path):
    path = os.path.abspath(path)
    while True:
        if os.path.isdir(os.path.join(path, "vilan", "std")) and os.path.isdir(os.path.join(path, "crates")):
            return True
        parent = os.path.dirname(path)
        if parent == path:
            return False
        path = parent


def loadavg():
    with open("/proc/loadavg") as handle:
        return float(handle.read().split()[0])


def t3_compare(options, sources):
    env_base, env_tip = side_envs(options)
    base_copy, tip_copy = sources["base"]["copy"], sources["tip"]["copy"]
    counter = resolve_counter("auto")
    runs = {"base": [], "tip": []}
    sides = (("base", options.base, env_base, base_copy), ("tip", options.tip, env_tip, tip_copy))
    for _, binary, env, copy in sides:
        perf_count.measure([binary, "check", "."], cwd=copy, env=env, counter=counter)  # warm-up each
    loads = [loadavg()]
    for _ in range(options.runs):
        for name, binary, env, copy in sides:
            runs[name].append(perf_count.measure([binary, "check", "."], cwd=copy, env=env, counter=counter))
        loads.append(loadavg())
    summary = {}
    for name, samples in runs.items():
        summary[name] = {
            "cpu_s": statistics.median(s["cpu_s"] for s in samples),
            "peak_rss_kb": max(s["peak_rss_kb"] or 0 for s in samples),
            "instructions": statistics.median(s["instructions"] for s in samples),
            "exit": samples[-1]["exit"],
        }
    ratio = {key: summary["tip"][key] / summary["base"][key] for key in ("cpu_s", "peak_rss_kb", "instructions")}
    red, notes = [], []
    load = max(loads)
    if load > options.max_load:
        red.append(f"loadavg {load:.1f} > {options.max_load}: the CPU verdict is not trusted (Q3) — re-run quiet")
    if ratio["cpu_s"] > options.threshold:
        red.append(f"kolt check CPU x{ratio['cpu_s']:.3f}")
    if ratio["peak_rss_kb"] > options.threshold:
        red.append(f"kolt check peak RSS x{ratio['peak_rss_kb']:.3f}")
    if sources["different"]:
        notes.append(different_sources_note(sources))
        # The checks were clean before the measurement (`refuse_unchecked_sources`); a run that then fails
        # is still a refusal, never a ratio.
        for name in ("base", "tip"):
            if summary[name]["exit"] != 0:
                red.append(f"the {name}'s check of its own source exited {summary[name]['exit']} during the "
                           "measurement")
    elif summary["base"]["exit"] != summary["tip"]["exit"]:
        notes.append("the two compilers disagree on the program (exit codes): the comparison covers different work")
    return {"kolt": sources["kolt"], "sources": {side: sources[side]["label"] for side in ("base", "tip")},
            "different_sources": sources["different"], "counter": counter, "load_max": load, "runs": options.runs, "base": summary["base"],
            "tip": summary["tip"], "ratio": ratio, "threshold": options.threshold, "red": red, "notes": notes}


def e121_rows(data, lsp_json):
    """E121's targets on the tip's LSP harness rows (Q9). Each row reports red until it has been green at
    two consecutive seals; from then it blocks. Returns (rows, blocking_red)."""
    if not lsp_json:
        return [], []
    with open(lsp_json) as handle:
        harness = json.load(handle)
    rows, blocking = [], []
    for target in data["e121"]:
        samples = harness.get("edits", {}).get(target["scenario"], [])
        if not samples:
            rows.append({"scenario": target["scenario"], "state": "absent"})
            continue
        metric = target.get("metric", "diagnostics_cpu_ms")
        median = statistics.median(s[metric] for s in samples)
        green = median <= target["target_ms"]
        state = "green" if green else ("RED (blocking)" if target.get("blocking") else "red (reporting)")
        rows.append({"scenario": target["scenario"], "metric": metric, "median_ms": median,
                     "target_ms": target["target_ms"], "state": state,
                     "green_seals": target.get("green_seals", 0)})
        if not green and target.get("blocking"):
            blocking.append(f"E121 {target['scenario']}: {median:.0f} ms > {target['target_ms']} ms")
    return rows, blocking


def advance_e121(data, rows):
    """After a seal: count consecutive green seals per row; the second makes it blocking (Q9)."""
    by_name = {(row["scenario"], row.get("metric")): row for row in rows}
    for target in data["e121"]:
        row = by_name.get((target["scenario"], target.get("metric", "diagnostics_cpu_ms")))
        if not row or "median_ms" not in row:
            continue
        if row["state"] == "green":
            target["green_seals"] = target.get("green_seals", 0) + 1
            if target["green_seals"] >= 2:
                target["blocking"] = True
        elif not target.get("blocking"):
            target["green_seals"] = 0


def lsp_compare(base_json, tip_json, threshold):
    """The LSP harness's rows tip against base, each row's median CPU to diagnostics (find 4)."""
    with open(base_json) as handle:
        base = json.load(handle)
    with open(tip_json) as handle:
        tip = json.load(handle)
    rows, red = [], []
    for scenario, samples in tip.get("edits", {}).items():
        before = base.get("edits", {}).get(scenario)
        if not before or not samples:
            continue
        b = statistics.median(s["diagnostics_cpu_ms"] for s in before)
        t = statistics.median(s["diagnostics_cpu_ms"] for s in samples)
        ratio = t / b if b else 1.0
        rows.append({"scenario": scenario, "base_ms": b, "tip_ms": t, "ratio": ratio})
        if ratio > threshold:
            red.append(f"LSP {scenario}: CPU to diagnostics x{ratio:.2f}")
    return rows, red


def lsp_source_notes(base_json, tip_json):
    """The LSP harness's two runs replayed one edit script; say so when they replayed it over different
    sources (`scripts/lsp-latency.py --source`, across a breaking release)."""
    with open(base_json) as handle:
        base = json.load(handle).get("source")
    with open(tip_json) as handle:
        tip = json.load(handle).get("source")
    if base == tip:
        return []
    return [f"the LSP harness replayed its edit script over DIFFERENT sources: the base {describe_source(base)}, "
            f"the tip {describe_source(tip)} — a breaking release; each row compares the same edit in the same "
            "app before and after its migration"]


def describe_source(source):
    if not source:
        return "(unrecorded)"
    if source.get("kind") == "prepared":
        return f"the prepared tree {source.get('path')}"
    return f"kolt@{str(source.get('sha', ''))[:8]}"


def command_seal(options):
    data = load_budgets(options.budgets)
    sha = options.sha or subprocess.run(["git", "-C", REPO, "rev-parse", "HEAD"], capture_output=True,
                                        text=True).stdout.strip()
    if options.tip_kolt and not options.kolt:
        sys.exit("perf_gate: --tip-kolt gives the TIP its own source; the base still needs --kolt (and --commit)")
    scratch = tempfile.mkdtemp(prefix="perf-seal-", dir=options.scratch)
    try:
        print(f"seal perf verdict for {sha[:10]} (load {loadavg():.1f})")
        sources = prepare_sources(options, scratch) if options.kolt else None
        if sources:
            refusals = refuse_unchecked_sources(options, sources)
            if refusals:
                for refusal in refusals:
                    print(f"  REFUSED  {refusal}")
                print("PERF VERDICT: REFUSED — across a breaking release each side must check its own source "
                      "with no errors; nothing was measured and no verdict was written")
                return 1
            if sources["different"]:
                print(f"  NOTE  {different_sources_note(sources)}")
        counter, results = measure_all(options.tip, data, options.work, "auto")
        t2_lines, t2_red = judge(data, results, counter, options.klass)
        print("\n".join(t2_lines))
        t3 = t3_compare(options, sources) if sources else None
        if t3:
            label = "kolt" if not t3["different_sources"] else "kolt (base and tip on different sources)"
            print(f"  T3 {label} @{t3['kolt']}: CPU x{t3['ratio']['cpu_s']:.3f}  "
                  f"RSS x{t3['ratio']['peak_rss_kb']:.3f}  instructions x{t3['ratio']['instructions']:.3f}  "
                  f"(load max {t3['load_max']:.1f})")
        lsp_rows, lsp_red, lsp_notes = ([], [], [])
        if options.lsp_json:
            lsp_rows, lsp_red = lsp_compare(options.lsp_json[0], options.lsp_json[1], options.threshold)
            lsp_notes = lsp_source_notes(options.lsp_json[0], options.lsp_json[1])
            for note in lsp_notes:
                print(f"  NOTE  {note}")
        e121, e121_red = e121_rows(data, options.lsp_json[1] if options.lsp_json else None)
        # §6.4: the report's first line names the most expensive phase — a SHARE of thread CPU on the
        # generated app, which a load moves far less than it moves the absolute figures.
        phases = phase_split(options.tip, subject_dir("genapp:46", options.work))
        bumps = [b for b in data["bump"]]
        owner_bumps = [b for b in bumps if b["ratio"] > 1.03 or b["subject"].startswith("kolt")]
        red = t2_red + (t3["red"] if t3 else []) + lsp_red + e121_red
        verdict = {
            "sha": sha, "date": datetime.date.today().isoformat(), "verdict": "red" if red else "green",
            "red": red, "load": loadavg(), "class": options.klass, "counter": counter,
            "t2": {"results": results, "judged": t2_lines}, "t3": t3, "lsp": lsp_rows, "lsp_notes": lsp_notes,
            "e121": e121,
            "bumps": bumps, "bumps_for_the_owner": owner_bumps, "genapp_phases_cpu_ms": phases,
        }
        os.makedirs(options.verdict_dir, exist_ok=True)
        out = os.path.join(options.verdict_dir, f"perf-{sha}.json")
        with open(out, "w") as handle:
            json.dump(verdict, handle, indent=1)
        print(f"wrote {out}")
        for bump in owner_bumps:
            print(f"  OWNER  bump {bump['subject']} x{bump['ratio']}: {bump['reason']} ({bump.get('item', '-')})")
        if options.advance:
            advance_e121(data, e121)
            write_budgets(data, options.budgets)
        print("PERF VERDICT: " + ("RED — " + "; ".join(red) if red else "green"))
        return 1 if red else 0
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


def command_e121(options):
    """E121's rows alone, on one LSP harness JSON (`scripts/lsp-latency.py --json`): print each row's state
    and, with `--advance`, count this seal toward the two-green rule and write `perf/budgets.toml`. Exit 1
    only when a BLOCKING row is red (Q9) — a row still reporting never refuses."""
    data = load_budgets(options.budgets)
    rows, blocking = e121_rows(data, options.lsp_json)
    for row in rows:
        if "median_ms" in row:
            print(f"  {row['state']:16} {row['scenario']} ({row['metric']}): {row['median_ms']:.0f} ms "
                  f"against {row['target_ms']} ms (green at {row['green_seals']} consecutive seals)")
        else:
            print(f"  absent           {row['scenario']}: the harness has no such row")
    if options.advance:
        advance_e121(data, rows)
        write_budgets(data, options.budgets)
    if blocking:
        print("E121 VERDICT: RED — " + "; ".join(blocking))
        return 1
    print("E121 VERDICT: green (no blocking row is red)")
    return 0


# ---------------------------------------------------------------------------- S4's calibration


def phase_split(vilan, directory):
    """Thread-CPU per top-level phase, summed over a check's analyses, from `VILAN_PHASE_TIMING`."""
    env = dict(os.environ, VILAN_PHASE_TIMING="1")
    vilan = os.path.abspath(vilan) if os.sep in vilan else vilan
    run = subprocess.run([vilan, "check", "."], cwd=directory, env=env, capture_output=True, text=True)
    phases = {}
    for line in run.stderr.splitlines():
        if not line.startswith("[vilan phase] ") or "resolve_world" in line or "macro-worlds" in line:
            continue
        words = line[len("[vilan phase] "):].split()
        for name, value in zip(words[::2], words[1::2]):
            # `<wall>ms/<cpu>cpu` figures only; `post-passes` is the sum of the buckets beside it, and
            # const-lower/const-interp/dispatch-refine are slices THROUGH other buckets (the line says so).
            if "ms/" not in value or name in ("post-passes", "const-lower", "const-interp", "dispatch-refine"):
                continue
            cpu = value.split("/")[1].replace("cpu", "")
            try:
                phases[name] = phases.get(name, 0.0) + float(cpu)
            except ValueError:
                pass
    return phases


def command_calibrate(options):
    work = options.work
    genapp = subject_dir("genapp:46", work)
    scratch = tempfile.mkdtemp(prefix="perf-calibrate-", dir=options.scratch)
    try:
        if options.kolt_tree:
            # A prepared tree, as `seal --tip-kolt` takes one: no git is run in it, which is what a lane
            # that may only read kolt through a scratch copy needs (M112).
            copy, sha = prepare_tree(options.kolt_tree, scratch), "tree"
        else:
            copy, sha = prepare_kolt(options.kolt, options.commit, scratch)
        splits = {}
        for name, directory in (("genapp", genapp), (f"kolt@{sha}", copy)):
            phase_split(options.vilan, directory)  # warm-up
            splits[name] = phase_split(options.vilan, directory)
        names = sorted(set().union(*[set(s) for s in splits.values()]))
        totals = {key: sum(value.values()) or 1.0 for key, value in splits.items()}
        drift = []
        print(f"{'phase':18}" + "".join(f"{key:>16}" for key in splits))
        for phase in names:
            shares = [100 * splits[key].get(phase, 0.0) / totals[key] for key in splits]
            print(f"{phase:18}" + "".join(f"{share:15.1f}%" for share in shares))
            if abs(shares[0] - shares[1]) > 10:
                drift.append(f"{phase} {shares[0]:.1f}% vs {shares[1]:.1f}%")
        if drift:
            print("DRIFT > 10 points (file a tracker item to retune perf_genapp.py, §5): " + "; ".join(drift))
            return 1
        print("calibrated: every phase within 10 points of kolt's share")
        return 0
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


# ---------------------------------------------------------------------------- S7's report


def command_report(options):
    with open(options.verdict) as handle:
        verdict = json.load(handle)
    previous = None
    if options.previous:
        with open(options.previous) as handle:
            previous = json.load(handle)
    out = [f"# Performance report — {options.title or verdict['sha'][:10]}", ""]
    phases = verdict.get("genapp_phases_cpu_ms") or {}
    if phases:
        total = sum(phases.values()) or 1.0
        name, cost = max(phases.items(), key=lambda item: item[1])
        out += [f"**The most expensive phase is `{name}`: {100 * cost / total:.0f}% of the generated app's "
                f"thread CPU** — the first place to optimize next.", ""]
    t3 = verdict.get("t3")
    if t3:
        out.append(f"Verdict **{verdict['verdict']}** at `{verdict['sha'][:10]}`, {verdict['date']}, load "
                   f"{verdict['load']:.1f}. kolt `vilan check`: CPU x{t3['ratio']['cpu_s']:.3f}, peak RSS "
                   f"x{t3['ratio']['peak_rss_kb']:.3f}, instructions x{t3['ratio']['instructions']:.3f} against "
                   f"the previous release.")
        for note in t3.get("notes", []):
            out += ["", f"> **Note:** {note}"]
    for note in verdict.get("lsp_notes", []):
        out += ["", f"> **Note:** {note}"]
    out += ["", f"## T2 — instruction counts ({verdict['counter']}, class {verdict['class']})", "",
            "| subject | count | previous | ratio |", "|---|---:|---:|---:|"]
    for subject, result in verdict["t2"]["results"].items():
        before = (previous or {}).get("t2", {}).get("results", {}).get(subject)
        if before:
            out.append(f"| {subject} | {result['instructions']:,} | {before['instructions']:,} | "
                       f"x{result['instructions'] / before['instructions']:.3f} |")
        else:
            out.append(f"| {subject} | {result['instructions']:,} | — | — |")
    if verdict.get("e121"):
        out += ["", "## E121 — the editor's targets", "", "| edit | CPU to diagnostics | target | state |",
                "|---|---:|---:|---|"]
        for row in verdict["e121"]:
            if "median_ms" in row:
                out.append(f"| {row['scenario']} ({row['metric']}) | {row['median_ms']:.0f} ms | "
                           f"{row['target_ms']} ms | {row['state']} |")
    plain = {s: r for s, r in verdict["t2"]["results"].items() if s.startswith("plain:")}
    if len(plain) >= 2:
        (small, a), (large, b) = sorted(plain.items(), key=lambda item: int(item[0].split(":")[1]))[:2]
        out += ["", f"Growth: {small} -> {large}: x{b['instructions'] / a['instructions']:.3f} per doubling "
                f"(the gate is x2.3)."]
    if verdict.get("bumps"):
        out += ["", "## Bumps since the last release", ""]
        out += [f"- {b['subject']} x{b['ratio']}: {b['reason']} ({b.get('item', '-')})" for b in verdict["bumps"]]
    if options.top:
        out += ["", "## Where the time goes (callgrind, profiling build)", "", "```", open(options.top).read().rstrip(),
                "```"]
    with open(options.out, "w") as handle:
        handle.write("\n".join(out) + "\n")
    print(f"wrote {options.out}")
    return 0


# ---------------------------------------------------------------------------- CLI


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--budgets", default=BUDGETS)
    parser.add_argument("--work", default=os.path.join(REPO, "target", "perf-gate"),
                        help="where generated subjects live (fixed paths; default target/perf-gate)")
    parser.add_argument("--scratch", default=None, help="parent of temporary copies (default: the system temp dir)")
    sub = parser.add_subparsers(dest="command", required=True)

    def add_class(p):
        p.add_argument("--class", dest="klass", default=os.environ.get("VILAN_PERF_CLASS", "local"),
                       help="the machine class whose ceilings apply (reference, ci, ...; $VILAN_PERF_CLASS)")

    p = sub.add_parser("measure")
    p.add_argument("--vilan", required=True)
    p.add_argument("--counter", default="auto", choices=["auto", "instructions:u", "callgrind"])
    p.add_argument("--subject", action="append")
    p.add_argument("--json")
    add_class(p)
    p.set_defaults(run=command_measure)

    p = sub.add_parser("gate")
    p.add_argument("--vilan", required=True)
    p.add_argument("--counter", default="auto", choices=["auto", "instructions:u", "callgrind"])
    p.add_argument("--json")
    add_class(p)
    p.set_defaults(run=command_gate)

    p = sub.add_parser("ratchet")
    p.add_argument("--from", dest="source", required=True)
    p.add_argument("--release", action="store_true")
    p.add_argument("--stamp")
    p.add_argument("--dry-run", action="store_true")
    p.set_defaults(run=command_ratchet)

    p = sub.add_parser("seal")
    p.add_argument("--tip", "--vilan", dest="tip", required=True)
    p.add_argument("--base", required=True)
    p.add_argument("--tip-std")
    p.add_argument("--base-std", help="VILAN_STD for the base (needed when the copy sits inside a checkout)")
    p.add_argument("--kolt", help="the base's source: a kolt checkout, archived at --commit")
    p.add_argument("--commit", default="HEAD")
    p.add_argument("--tip-kolt", help="the TIP's own source across a breaking release: a prepared, already-migrated "
                                      "kolt tree, used as it stands (no git); the base keeps --kolt/--commit")
    p.add_argument("--runs", type=int, default=5)
    p.add_argument("--threshold", type=float, default=1.10)
    p.add_argument("--max-load", type=float, default=2.0)
    p.add_argument("--lsp-json", nargs=2, metavar=("BASE", "TIP"))
    p.add_argument("--sha")
    p.add_argument("--verdict-dir", default=os.environ.get("VILAN_PERF_VERDICTS", DEFAULT_VERDICTS))
    p.add_argument("--advance", action="store_true", help="count this seal toward E121's two-green rule")
    add_class(p)
    p.set_defaults(run=command_seal)

    p = sub.add_parser("e121")
    p.add_argument("--lsp-json", required=True, help="the tip's `scripts/lsp-latency.py --json` output")
    p.add_argument("--advance", action="store_true", help="count this seal toward the two-green rule")
    p.set_defaults(run=command_e121)

    p = sub.add_parser("calibrate")
    p.add_argument("--vilan", required=True)
    source = p.add_mutually_exclusive_group(required=True)
    source.add_argument("--kolt", help="a kolt checkout, archived at --commit")
    source.add_argument("--kolt-tree", help="a prepared kolt tree, used as it stands (no git)")
    p.add_argument("--commit", default="HEAD")
    p.set_defaults(run=command_calibrate)

    p = sub.add_parser("report")
    p.add_argument("--verdict", required=True)
    p.add_argument("--previous")
    p.add_argument("--top", help="a callgrind top-N text to append")
    p.add_argument("--title")
    p.add_argument("--out", required=True)
    p.set_defaults(run=command_report)

    options = parser.parse_args()
    os.makedirs(options.work, exist_ok=True)
    sys.exit(options.run(options) or 0)


if __name__ == "__main__":
    main()
