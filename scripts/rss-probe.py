#!/usr/bin/env python3
"""rss-probe.py — peak resident memory of `vilan check .` in a directory, one FRESH process per run.

    scripts/rss-probe.py <vilan-binary> <dir> [runs]        # runs defaults to 3

Prints each run's peak RSS (KB, sorted) and the MEDIAN, with the median run's user CPU and the exit code:

    runs: 262144KB 263320KB 291840KB | median peakRSS=263320 KB user=2.41s exit=0

The reading a lane's "kolt's cold check within 3% of the base in RSS" gate takes (Order 49: a hash table's
doubling is a cliff, and the TypeTable's was +46 MB on one threshold). Each run is its own child, reaped by
`wait4(2)`, so its `ru_maxrss` is that run's alone - the probe's first form read `RUSAGE_CHILDREN`, the MAX over
every child ever reaped, and called it the median. The median, not the max: the first run under a stray load
reads high, and the gate compares at 3%.

`ru_maxrss` includes the forking process's resident set (~10 MB for Python), a floor that is invisible next to a
compiler run; do not point this at a command that allocates less than that. The seal's T3 kolt row reports the
same figure the same way (`perf_gate.py seal`, through `perf_count.peak_rss` / `perf_count.measure`).

Linux only. Python >= 3.11."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import perf_count  # noqa: E402


def probe(binary, directory, runs):
    """`runs` fresh runs of `binary check .` in `directory`: [(peak_rss_kb, cpu_s, exit)], sorted by RSS."""
    rows = []
    for _ in range(runs):
        run = perf_count.peak_rss([binary, "check", "."], cwd=directory)
        rows.append((run["peak_rss_kb"], run["cpu_s"], run["exit"]))
    rows.sort()
    return rows


def main(argv):
    if len(argv) not in (3, 4):
        sys.exit(__doc__)
    binary, directory = argv[1], argv[2]
    runs = int(argv[3]) if len(argv) == 4 else 3
    if not os.path.isdir(directory):
        sys.exit(f"rss-probe: {directory} is not a directory")
    if not os.access(binary, os.X_OK) and not any(
        os.access(os.path.join(entry, binary), os.X_OK) for entry in os.environ.get("PATH", "").split(os.pathsep)
    ):
        sys.exit(f"rss-probe: {binary} is not an executable")
    if runs < 1:
        sys.exit("rss-probe: runs must be at least 1")
    rows = probe(binary, directory, runs)
    median = rows[len(rows) // 2]
    print("runs:", " ".join(f"{kb}KB" for kb, _, _ in rows),
          f"| median peakRSS={median[0]} KB user={median[1]}s exit={median[2]}")
    return 0 if all(code == 0 for _, _, code in rows) else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
