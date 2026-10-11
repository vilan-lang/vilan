#!/usr/bin/env python3
"""perf_count.py — run one command and report its DETERMINISTIC cost: user-space instructions and peak RSS.

    scripts/perf_count.py [--json] [--cwd DIR] [--env K=V ...] -- vilan check .

The instrument behind M105's gates (`proposals/projects/vilan/proposal/performance-gates.md` §2.2):

- `instructions:u` from the hardware counter, through `perf_event_open(2)` on the child with
  `enable_on_exec` and `inherit` (so every thread the compiler spawns is counted, and nothing of this
  script is). Under load the count moves by parts per million where wall and CPU time move by 2x.
- `callgrind` Ir as the fallback where the machine exposes no PMU (a VM without a virtual PMU, some CI
  runners): `--counter callgrind`, or `--counter auto` (the default) falls back by itself. A budget names
  its counter, because the two differ by up to ~2.6% (callgrind counts the dynamic loader too).
- peak RSS from `wait4(2)`'s per-child rusage — that child's own high-water mark, never
  `RUSAGE_CHILDREN` (the maximum over every child reaped so far, which made a ratio vacuous).
- CPU time (user+sys) is reported too, as information: it is a claim about the machine's load.

Linux only. Used as a library by `perf_gate.py` (`measure(argv, cwd, env, counter)`)."""
import argparse
import ctypes
import ctypes.util
import json
import os
import platform
import re
import struct
import subprocess
import sys
import tempfile

_PERF_EVENT_OPEN = {"x86_64": 298, "aarch64": 241}


class _PerfEventAttr(ctypes.Structure):
    _fields_ = [
        ("type", ctypes.c_uint32), ("size", ctypes.c_uint32), ("config", ctypes.c_uint64),
        ("sample_period", ctypes.c_uint64), ("sample_type", ctypes.c_uint64),
        ("read_format", ctypes.c_uint64), ("flags", ctypes.c_uint64),
        ("wakeup_events", ctypes.c_uint32), ("bp_type", ctypes.c_uint32),
        ("config1", ctypes.c_uint64), ("config2", ctypes.c_uint64),
        ("branch_sample_type", ctypes.c_uint64), ("sample_regs_user", ctypes.c_uint64),
        ("sample_stack_user", ctypes.c_uint32), ("clockid", ctypes.c_int32),
        ("sample_regs_intr", ctypes.c_uint64), ("aux_watermark", ctypes.c_uint32),
        ("sample_max_stack", ctypes.c_uint16), ("reserved", ctypes.c_uint16),
    ]


# perf_event_attr flag bits (linux/perf_event.h): disabled 0, inherit 1, exclude_kernel 5, exclude_hv 6,
# enable_on_exec 12.
_FLAGS = (1 << 0) | (1 << 1) | (1 << 5) | (1 << 6) | (1 << 12)


def _open_counter(pid):
    number = _PERF_EVENT_OPEN.get(platform.machine())
    if number is None:
        return None
    libc = ctypes.CDLL(ctypes.util.find_library("c"), use_errno=True)
    attr = _PerfEventAttr()
    attr.type = 0  # PERF_TYPE_HARDWARE
    attr.size = ctypes.sizeof(_PerfEventAttr)
    attr.config = 1  # PERF_COUNT_HW_INSTRUCTIONS
    attr.flags = _FLAGS
    fd = libc.syscall(number, ctypes.byref(attr), pid, -1, -1, 0)
    return fd if fd >= 0 else None


def hardware_counter_available():
    """Whether this machine lets a process count its own children's user-space instructions."""
    fd = _open_counter(0)
    if fd is None:
        return False
    os.close(fd)
    return True


def _measure_hardware(argv, cwd, env, stdout):
    ready_read, ready_write = os.pipe()
    child = os.fork()
    if child == 0:  # the child waits until the counter is attached, then becomes the command
        try:
            os.close(ready_write)
            os.read(ready_read, 1)
            if cwd:
                os.chdir(cwd)
            if stdout is not None:
                os.dup2(stdout, 1)
                os.dup2(stdout, 2)
            os.execvpe(argv[0], argv, env)
        finally:
            os._exit(127)
    os.close(ready_read)
    fd = _open_counter(child)
    if fd is None:
        os.kill(child, 9)
        os.waitpid(child, 0)
        raise OSError("perf_event_open(instructions:u) refused")
    os.write(ready_write, b"x")
    os.close(ready_write)
    _, status, usage = os.wait4(child, 0)
    instructions = struct.unpack("q", os.read(fd, 8))[0]
    os.close(fd)
    return instructions, status, usage


def _measure_callgrind(argv, cwd, env, stdout):
    with tempfile.TemporaryDirectory(prefix="perf-count-") as scratch:
        out = os.path.join(scratch, "callgrind.out")
        command = ["valgrind", "--tool=callgrind", f"--callgrind-out-file={out}", "--trace-children=no", *argv]
        process = subprocess.Popen(command, cwd=cwd, env=env, stdout=stdout or subprocess.DEVNULL,
                                   stderr=stdout or subprocess.DEVNULL)
        _, status, usage = os.wait4(process.pid, 0)
        instructions = None
        with open(out) as handle:
            for line in handle:
                match = re.match(r"^(?:summary|totals):\s+(\d+)", line)
                if match:
                    instructions = int(match.group(1))
        if instructions is None:
            raise OSError("callgrind wrote no totals")
        return instructions, status, usage


def measure(argv, cwd=None, env=None, counter="auto", log=None):
    """Run `argv` once; return {counter, instructions, peak_rss_kb, cpu_s, exit}. `log` is a path that
    receives the command's stdout and stderr (default: discarded)."""
    env = dict(os.environ if env is None else env)
    # The child changes directory before it execs, so a relative program path is resolved here, first.
    argv = [os.path.abspath(argv[0]) if os.sep in argv[0] else argv[0], *argv[1:]]
    if counter == "auto":
        counter = "instructions:u" if hardware_counter_available() else "callgrind"
    stdout = os.open(log, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644) if log else os.open(os.devnull, os.O_WRONLY)
    try:
        if counter == "instructions:u":
            instructions, status, usage = _measure_hardware(argv, cwd, env, stdout)
        elif counter == "callgrind":
            instructions, status, usage = _measure_callgrind(argv, cwd, env, stdout)
        else:
            raise ValueError(f"unknown counter {counter!r}")
    finally:
        os.close(stdout)
    return {
        "counter": counter,
        "instructions": instructions,
        # Under callgrind this is valgrind's own peak, not the compiler's: report it, never budget it.
        "peak_rss_kb": usage.ru_maxrss if counter == "instructions:u" else None,
        "cpu_s": round(usage.ru_utime + usage.ru_stime, 3),
        "exit": os.waitstatus_to_exitcode(status),
    }


def peak_rss(argv, cwd=None, env=None):
    """Run `argv` once, as a FRESH process, and return {peak_rss_kb, cpu_s, exit} - no counter, so it works where
    `perf_event_open` is refused. `wait4`'s rusage is that child's alone: `RUSAGE_CHILDREN` is the max over EVERY
    child the caller ever reaped, which is how scripts/rss-probe.py's first form read the MAX of its runs as each
    run's figure (Order 49). `ru_maxrss` also folds in the forking process's resident set (~10 MB for Python), a
    floor that is invisible next to any compiler run but is the reading for a command that allocates nothing."""
    env = dict(os.environ if env is None else env)
    argv = [os.path.abspath(argv[0]) if os.sep in argv[0] else argv[0], *argv[1:]]
    process = subprocess.Popen(argv, cwd=cwd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    _, status, usage = os.wait4(process.pid, 0)
    process.returncode = os.waitstatus_to_exitcode(status)  # reaped here; keep Popen from waiting again
    return {"peak_rss_kb": usage.ru_maxrss, "cpu_s": round(usage.ru_utime + usage.ru_stime, 3),
            "exit": process.returncode}


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--cwd")
    parser.add_argument("--counter", default="auto", choices=["auto", "instructions:u", "callgrind"])
    parser.add_argument("--env", action="append", default=[], metavar="K=V")
    parser.add_argument("--log", help="write the command's output here (default: discarded)")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    options = parser.parse_args()
    command = options.command[1:] if options.command[:1] == ["--"] else options.command
    if not command:
        parser.error("no command")
    env = dict(os.environ)
    for pair in options.env:
        key, _, value = pair.partition("=")
        env[key] = value
    result = measure(command, options.cwd, env, options.counter, options.log)
    if options.json:
        print(json.dumps(result))
    else:
        rss = f"{result['peak_rss_kb'] / 1024:.1f} MB" if result["peak_rss_kb"] else "n/a"
        print(f"{result['counter']} {result['instructions']:,}  peak RSS {rss}  CPU {result['cpu_s']} s  exit {result['exit']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
