#!/usr/bin/env python3
"""E236: the language server's latency on a real package, under document edits.

Drives a `vilan-lsp` process over stdio (JSON-RPC, the protocol an editor
speaks) through a FIXED edit script on a scratch COPY of a package — kolt by
default, at a pinned commit — and prints one table row per edit:

  - time to diagnostics: the LAST `publishDiagnostics` for the edited file
    before the server goes idle — the analysis's; since E242 the first one is
    the immediate repaint of the followed diagnostics — (wall, debounce
    included: the server waits `clamp(0.3 x last analysis, 150, 500)` ms
    before it starts), the CPU spent up to it, and the CPU the server process
    spent from the edit until it went idle again;
  - the keystroke path: hover, completion, inlay hints and semantic tokens
    asked IMMEDIATELY after the edit, before any analysis has landed — E121's
    <10 ms path (wall: the analysis is running beside them, so their own CPU
    cannot be separated from its);
  - the same four requests once the server is idle (CPU, summed over the
    server's live threads, so a request is measured in nanoseconds, not in
    10 ms clock ticks);
  - peak resident memory (VmHWM) after the edit;
  - user-space INSTRUCTIONS from the edit to idle (E244, perf-b-45's counter):
    a hardware counter attached to every server thread with `inherit`, so the
    analysis threads it spawns later are counted too. Instructions do not move
    with the load average the way CPU time does on this host; `-` where
    `perf_event_open` is refused (`/proc/sys/kernel/perf_event_paranoid`);
  - the analyses an edit ran, counted from the server's `VILAN_PHASE_TIMING`
    lines, where the server prints them.

CPU time, not wall, is the figure to compare: a machine running other work
stretches wall time and leaves CPU time alone (E121's measurement rule — and
record the load average beside every number, which this prints).

A KEYSTROKE-PLUS-PAUSE row (E244) types the leaf keystroke and then RESTS
until the server has been quiet for three seconds: every other row resumes
after 0.4 s of quiet, which is shorter than the dead-code clock's pause, so
the analyses a real pause sets off never show there.

The edits (`SCENARIOS` below): a keystroke in a leaf file, one in the widely
imported `shared.vl`, one in the client model `model.vl` (whose edit
re-analyses the whole client world), one inside a `css { }` block, and an edit that breaks the
parse and then repairs it. Each keystroke is undone after it is measured, so
every repetition starts from the same text. One document is open at a time.

  scripts/lsp-latency.py --kolt ~/code/kolt                    # installed vilan-lsp
  scripts/lsp-latency.py --kolt ~/code/kolt --lsp target/release/vilan-lsp
  scripts/lsp-latency.py --kolt ~/code/kolt --commit 984a1dfb --runs 7
  scripts/lsp-latency.py --kolt ~/code/kolt --callgrind out/ --scenario "parse break"
  scripts/lsp-latency.py --source /path/to/migrated-kolt --lsp target/release/vilan-lsp

`--source DIR` replays the script over a PREPARED tree used as it stands (copied
to the scratch location; no git is run in it) instead of `--kolt` at a commit:
across a breaking release the tip's run takes the migrated kolt while the base's
keeps the release's commit. Every anchor of the chosen scenarios is looked up in
the copy BEFORE the server starts, and the run refuses — naming each one — when
an anchor is missing, or when an edit's anchor occurs more than once (the edit
would land on the first, which may not be the place it means). `--json` records
which source the run replayed, so the seal can say the two sides differed.

`--callgrind DIR` runs the server under valgrind's callgrind with instrumentation
OFF, switches it on for ONE scenario's measured edit only (`callgrind_control
-i on`), and prints `callgrind_annotate`'s top offenders. Use a binary built
with symbols (`cargo build --profile profiling -p vilan-lsp`); the release
profile strips them.

Standard library only. Linux only (it reads /proc/<pid>/). The server's PID is
recorded at spawn and the harness kills THAT process, never by name.
"""

import argparse
import ctypes
import ctypes.util
import struct
import json
import os
import queue
import shutil
import statistics
import subprocess
import sys
import tarfile
import io
import threading
import time
from pathlib import Path

CLOCK_TICKS = os.sysconf("SC_CLK_TCK")

# The fixed edit script. `edit` is (anchor, offset into it): the edit inserts
# `text` there. `hover` and `completion` are (anchor, offset) too, resolved in
# the CURRENT text of the document each time they are asked, so an edit on the
# same line does not move them — and so they must be found in the EDITED text as
# well as the original one (N150; `anchor_problems` checks both). `references`
# is asked once every edit is undone, against the original text. `repair`
# measures the undo as its own row.
SCENARIOS = [
    {
        "name": "leaf keystroke",
        "file": "src/views.vl",
        "edit": ("\tlet theme_modal = create_theme_modal();", 1),
        "text": " ",
        "hover": ("create_theme_modal();", 3),
        "completion": ("get_prefs().theme.derive", len("get_prefs().")),
    },
    {
        # E244: the same keystroke, then a REAL pause — the dead-code clock's
        # union analyses run, which a row resuming after 0.4 s never sees.
        "name": "leaf keystroke + pause",
        "file": "src/views.vl",
        "edit": ("\tlet theme_modal = create_theme_modal();", 1),
        "text": " ",
        "pause": 3.0,
        "hover": ("create_theme_modal();", 3),
        "completion": ("get_prefs().theme.derive", len("get_prefs().")),
    },
    {
        # M110 S1's acceptance row: the same keystroke with the ENTRY open beside
        # `views.vl`, so the file is served from the client world (M104's world
        # mode) — the row the hot-set world exists for (~14.3 G before it).
        "name": "leaf keystroke, world mode",
        "file": "src/views.vl",
        "also_open": ["src/client.vl"],
        "edit": ("\tlet theme_modal = create_theme_modal();", 1),
        "text": " ",
        "hover": ("create_theme_modal();", 3),
        "completion": ("get_prefs().theme.derive", len("get_prefs().")),
    },
    {
        # N145: anchored on `UserId`'s `Hashable` impl, which kolt@984a1dfb (the
        # v0.44.0 seal's base) and the v0.44.0-migrated tree both hold once. The
        # first anchors (`Transient`'s `ready`/`latest`) left shared.vl with the
        # A150 migration, and the preflight refused the scenario on every tree
        # after it. N150: the completion anchor is the text AFTER the indent —
        # the edit inserts its space inside `\t\t`, and the keystroke-path
        # requests are resolved in the EDITED buffer, so an anchor spanning the
        # insertion point stopped every run mid-scenario.
        "name": "shared.vl keystroke",
        "file": "src/shared.vl",
        "edit": ("\t\tself.uuid.hash()", 2),
        "text": " ",
        "hover": ("self.uuid.hash()", 10),
        "completion": ("self.uuid.hash()", len("self.")),
    },
    {
        # The owner's case: an edit to the client MODEL re-analyses the whole
        # client world — module reuse is all-or-nothing per world (0/82).
        "name": "model.vl keystroke",
        "file": "src/model.vl",
        "edit": ("\t\tget_client().get_safe()!.create_channel(name)", 2),
        "text": " ",
        "hover": ("create_channel(name)", 3),
        "completion": ("get_client().get_safe()!.create_channel", len("get_client().")),
        # M104's hybrid: model.vl open ALONE answers Find References from its
        # entry's world, built on demand after an edit — `messages` is used in
        # `channel.vl`, which is not open.
        "references": ("fun messages(self)", 5),
    },
    {
        # The same edit with the files that import the model OPEN beside it,
        # as the owner's editor has them: an edit re-analyses its open
        # dependents too, and that is the cost the single-document row hides.
        "name": "model.vl keystroke, importers open",
        "file": "src/model.vl",
        "also_open": ["src/client.vl", "src/channel.vl", "src/views.vl"],
        "edit": ("\t\tget_client().get_safe()!.create_channel(name)", 2),
        "text": " ",
        "hover": ("create_channel(name)", 3),
        "completion": ("get_client().get_safe()!.create_channel", len("get_client().")),
    },
    {
        "name": "css keystroke",
        "file": "src/styles.vl",
        "edit": ("\twidth(size(4));", 1),
        "text": " ",
        "hover": ("min-width(size(4));", 11),
        "completion": ('style::element("placeholder")', len("style::")),
    },
    {
        "name": "parse break",
        "file": "src/channel.vl",
        "edit": ("\tlet messages = channel.messages();", 1),
        "text": "(",
        "repair": "parse repair",
        "hover": ("channel.messages();", 10),
        "completion": ("channel.messages();", len("channel.")),
    },
]

KEYSTROKE_REQUESTS = ["hover", "completion", "inlay", "tokens"]

# M110 S0's scripted session (`--session`): ten keystrokes in each of four
# places the owner types, with the client ENTRY open beside every file so the
# file is served from the client world (M104's world mode, the case incremental
# analysis is for). Each keystroke is one `didChange`, typed into the text the
# previous keystrokes left — the intermediate states are as broken as typing
# makes them, which is Q10's question — and the four places are undone between
# files so each starts from the tree as it stands. `anchor` is (text, offset
# into it); the keystrokes insert there, in order.
SESSION = [
    {
        "file": "src/views.vl",
        "anchor": ("\tlet theme_modal = create_theme_modal();", len("\tlet theme_modal = create_theme_modal();")),
        "keystrokes": list(" let x=1; "),
    },
    {
        "file": "src/theme.vl",
        "anchor": (
            "\tmut initial_theme = get_prefs().theme.get();",
            len("\tmut initial_theme = get_prefs().theme.get();"),
        ),
        "keystrokes": list(" let y=2; "),
    },
    {
        "file": "src/model.vl",
        "anchor": (
            "\tfun create(name: str): Result<u53, RpcError> {",
            len("\tfun create(name: str): Result<u53, RpcError> {"),
        ),
        "keystrokes": list(" let z=3; "),
    },
    {
        "file": "src/styles.vl",
        "anchor": ("\twidth(size(4));", len("\twidth(size(4));")),
        "keystrokes": [" ", "h", "e", "i", "g", "h", "t", "(", "size(4)", ");"],
    },
]
SESSION_COMPANIONS = ["src/client.vl"]


# --- /proc readings -----------------------------------------------------------


def cpu_ms(pid):
    """utime + stime of the whole process, dead threads included (clock ticks,
    so a 10 ms resolution: right for an analysis, too coarse for a request)."""
    with open(f"/proc/{pid}/stat") as handle:
        fields = handle.read().rsplit(")", 1)[1].split()
    return (int(fields[11]) + int(fields[12])) * 1000.0 / CLOCK_TICKS


def live_threads_ns(pid):
    """CPU nanoseconds summed over the process's LIVE threads (schedstat). Exact,
    but blind to a thread that exits inside the window — so it is read only
    across a request asked while the server is idle."""
    total = 0
    for task in os.listdir(f"/proc/{pid}/task"):
        try:
            with open(f"/proc/{pid}/task/{task}/schedstat") as handle:
                total += int(handle.read().split()[0])
        except (FileNotFoundError, ProcessLookupError, IndexError, ValueError):
            pass
    return total


def memory_kb(pid):
    readings = {}
    with open(f"/proc/{pid}/status") as handle:
        for line in handle:
            if line.startswith(("VmHWM:", "VmRSS:")):
                key, value = line.split(":", 1)
                readings[key] = int(value.split()[0])
    return readings


class InstructionCounter:
    """User-space instructions retired by a process and every thread it spawns
    after attachment (E244): one `perf_event_open` hardware counter per live
    thread, `inherit` set, so an analysis thread spawned later by any of them
    is counted — its count is folded into its parent's when it exits, which is
    why the counter is read at IDLE. `available` is false where the kernel
    refuses the counter; every reading is then `None`."""

    HW_INSTRUCTIONS = 1

    class _Attr(ctypes.Structure):
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

    def __init__(self, pid):
        self.fds = []
        self.available = False
        if not sys.platform.startswith("linux") or os.uname().machine != "x86_64":
            return
        libc = ctypes.CDLL(ctypes.util.find_library("c"), use_errno=True)
        try:
            for task in os.listdir(f"/proc/{pid}/task"):
                attr = self._Attr()
                attr.type = 0  # PERF_TYPE_HARDWARE
                attr.size = ctypes.sizeof(self._Attr)
                attr.config = self.HW_INSTRUCTIONS
                # inherit (bit 1), exclude_kernel (bit 5), exclude_hv (bit 6).
                attr.flags = (1 << 1) | (1 << 5) | (1 << 6)
                fd = libc.syscall(298, ctypes.byref(attr), int(task), -1, -1, 0)  # perf_event_open
                if fd < 0:
                    raise OSError(ctypes.get_errno(), "perf_event_open")
                self.fds.append(fd)
            self.available = True
        except OSError:
            for fd in self.fds:
                os.close(fd)
            self.fds = []

    def read(self):
        if not self.available:
            return None
        return sum(struct.unpack("q", os.read(fd, 8))[0] for fd in self.fds)


def load_average():
    with open("/proc/loadavg") as handle:
        return " ".join(handle.read().split()[:3])


# --- the protocol ---------------------------------------------------------------


class Server:
    """One `vilan-lsp` process and the reader thread that drains its stdout."""

    def __init__(self, command, cwd, env, stderr_path):
        self.stderr = open(stderr_path, "wb")
        self.log = open(Path(stderr_path).with_suffix(".log"), "w")
        self.process = subprocess.Popen(
            command,
            cwd=cwd,
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.stderr,
        )
        self.pid = self.process.pid
        self.next_id = 0
        self.instructions = InstructionCounter(-1)
        self.responses = {}
        self.response_ready = threading.Condition()
        self.publishes = []  # (time, uri, diagnostics)
        self.publish_ready = threading.Condition()
        self.write_lock = threading.Lock()
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _send(self, message):
        body = json.dumps(message).encode()
        with self.write_lock:
            self.process.stdin.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
            self.process.stdin.flush()

    def _read(self):
        stream = self.process.stdout
        while True:
            length = None
            while True:
                line = stream.readline()
                if not line:
                    return
                line = line.strip()
                if not line:
                    break
                if line.lower().startswith(b"content-length:"):
                    length = int(line.split(b":", 1)[1])
            if length is None:
                continue
            message = json.loads(stream.read(length))
            now = time.perf_counter()
            if "id" in message and "method" in message:
                # A server-to-client request (a refresh, a registration): the
                # harness accepts everything and answers null.
                self._send({"jsonrpc": "2.0", "id": message["id"], "result": None})
            elif "id" in message:
                with self.response_ready:
                    self.responses[message["id"]] = (now, message)
                    self.response_ready.notify_all()
            elif message.get("method") == "textDocument/publishDiagnostics":
                params = message["params"]
                # The process's CPU at the moment the publish arrived — what
                # the edit cost up to its diagnostics, without what follows.
                cpu = cpu_ms(self.pid)
                with self.publish_ready:
                    self.publishes.append((now, params["uri"], params["diagnostics"], cpu))
                    self.publish_ready.notify_all()
            elif message.get("method") == "window/logMessage":
                self.log.write(f"{now:.3f} {message['params'].get('message', '')}\n")
                self.log.flush()

    def stderr_mark(self):
        """Where the server's stderr ends now — `stderr_since` reads what
        follows it."""
        self.stderr.flush()
        try:
            return Path(self.stderr.name).stat().st_size
        except OSError:
            return 0

    def stderr_since(self, mark):
        self.stderr.flush()
        try:
            with open(self.stderr.name, "rb") as handle:
                handle.seek(mark)
                return handle.read().decode(errors="replace").splitlines()
        except OSError:
            return []

    def analyses_logged(self):
        """How many analyses the server has reported on stderr so far
        (`VILAN_PHASE_TIMING`'s `lsp-context` lines), or `None` before it
        has printed one."""
        self.stderr.flush()
        try:
            text = Path(self.stderr.name).read_text(errors="replace")
        except OSError:
            return None
        count = text.count("lsp-context")
        return count if count else None

    def notify(self, method, params):
        self._send({"jsonrpc": "2.0", "method": method, "params": params})

    def request(self, method, params, timeout=600):
        """Send, wait, and answer (result, wall seconds)."""
        self.next_id += 1
        request_id = self.next_id
        started = time.perf_counter()
        self._send({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})
        with self.response_ready:
            if not self.response_ready.wait_for(lambda: request_id in self.responses, timeout):
                raise TimeoutError(f"{method} timed out after {timeout}s")
            answered, message = self.responses.pop(request_id)
        if "error" in message:
            raise RuntimeError(f"{method}: {message['error']}")
        return message.get("result"), answered - started

    def wait_publish(self, uri, after, timeout=600):
        """The first publish for `uri` strictly after `after`."""
        def found():
            return next(
                (entry for entry in self.publishes if entry[0] > after and entry[1] == uri),
                None,
            )

        with self.publish_ready:
            if not self.publish_ready.wait_for(lambda: found() is not None, timeout):
                raise TimeoutError(f"no diagnostics for {uri} within {timeout}s")
            return found()

    def last_publish(self, uri, after):
        with self.publish_ready:
            entries = [entry for entry in self.publishes if entry[0] > after and entry[1] == uri]
        return entries[-1] if entries else None

    def settle(self, quiet_polls=4, interval=0.1, timeout=600):
        """Wait until the process has spent no CPU for `quiet_polls` polls in a
        row — no analysis, no post-pass, no publish in flight."""
        deadline = time.perf_counter() + timeout
        last = cpu_ms(self.pid)
        quiet = 0
        while quiet < quiet_polls:
            if time.perf_counter() > deadline:
                raise TimeoutError("the server never went idle")
            time.sleep(interval)
            now = cpu_ms(self.pid)
            quiet = quiet + 1 if now == last else 0
            last = now
        return time.perf_counter()

    def stop(self):
        try:
            self.request("shutdown", None, timeout=30)
            self.notify("exit", None)
            self.process.wait(timeout=30)
        except Exception:
            pass
        if self.process.poll() is None:
            # By PID: this process and no other.
            self.process.kill()
            self.process.wait()
        self.stderr.close()
        self.log.close()


# --- text positions ----------------------------------------------------------------


def position(text, offset):
    """An LSP position (UTF-16 columns) for a byte-free str offset."""
    line_start = text.rfind("\n", 0, offset) + 1
    line = text.count("\n", 0, offset)
    character = len(text[line_start:offset].encode("utf-16-le")) // 2
    return {"line": line, "character": character}


def anchor_offset(text, anchor, scenario, what):
    needle, delta = anchor
    at = text.find(needle)
    if at < 0:
        raise SystemExit(
            f"{scenario['name']}: the {what} anchor {needle!r} is not in {scenario['file']} "
            "- the edit script names text at the pinned commit; pass that --commit"
        )
    return at + delta


ANCHOR_KINDS = ("edit", "hover", "completion", "references")

# The anchors asked while the scenario's edit is still in the buffer (`measure_edit`'s keystroke-path and
# settled requests), which must therefore survive the edit.
EDITED_ANCHOR_KINDS = ("hover", "completion")


def anchor_problems(root, scenarios):
    """Every anchor of `scenarios` checked against the tree at `root` before anything runs: a missing
    anchor, an EDIT anchor that occurs more than once (the edit lands on the first occurrence, which may
    not be the place the script means), or a hover/completion anchor the scenario's own edit destroys —
    those are resolved in the EDITED buffer (N150: the `shared.vl` row's completion anchor spanned its
    insertion point, so the preflight passed and every run died mid-scenario). Answers the problems in
    words (empty: every anchor lands)."""
    problems = []
    for scenario in scenarios:
        path = root / scenario["file"]
        try:
            text = path.read_text()
        except OSError as error:
            problems.append(f"{scenario['name']}: cannot read {scenario['file']} ({error.strerror})")
            continue
        for kind in ANCHOR_KINDS:
            if kind not in scenario:
                continue
            needle = scenario[kind][0]
            count = text.count(needle)
            if count == 0:
                problems.append(f"{scenario['name']}: the {kind} anchor {needle!r} is not in {scenario['file']}")
            elif count > 1 and kind == "edit":
                problems.append(f"{scenario['name']}: the edit anchor {needle!r} occurs {count} times in "
                                f"{scenario['file']} - the edit would land on the first")
        edit_needle, edit_delta = scenario["edit"]
        at = text.find(edit_needle)
        if at < 0:
            continue
        edited = text[: at + edit_delta] + scenario["text"] + text[at + edit_delta :]
        for kind in EDITED_ANCHOR_KINDS:
            if kind not in scenario:
                continue
            needle = scenario[kind][0]
            if needle in text and needle not in edited:
                problems.append(f"{scenario['name']}: the {kind} anchor {needle!r} is not in {scenario['file']} "
                                f"after the scenario's edit - it is asked against the edited text")
    return problems


def end_position(text):
    return position(text, len(text))


# --- the run ----------------------------------------------------------------------------


class Document:
    def __init__(self, server, path):
        self.server = server
        self.uri = path.resolve().as_uri()
        self.text = path.read_text()
        self.version = 1

    def open(self):
        self.server.notify(
            "textDocument/didOpen",
            {"textDocument": {"uri": self.uri, "languageId": "vilan", "version": 1, "text": self.text}},
        )

    def close(self):
        self.server.notify("textDocument/didClose", {"textDocument": {"uri": self.uri}})

    def insert(self, offset, inserted):
        start = position(self.text, offset)
        self.text = self.text[:offset] + inserted + self.text[offset:]
        self._change(start, start, inserted)

    def delete(self, offset, length):
        start = position(self.text, offset)
        end = position(self.text, offset + length)
        self.text = self.text[:offset] + self.text[offset + length :]
        self._change(start, end, "")

    def _change(self, start, end, inserted):
        self.version += 1
        self.server.notify(
            "textDocument/didChange",
            {
                "textDocument": {"uri": self.uri, "version": self.version},
                "contentChanges": [{"range": {"start": start, "end": end}, "text": inserted}],
            },
        )

    def ask(self, kind, scenario):
        """One request of `kind` against the current text; answers wall seconds."""
        identifier = {"textDocument": {"uri": self.uri}}
        if kind == "hover":
            at = anchor_offset(self.text, scenario["hover"], scenario, "hover")
            _, wall = self.server.request("textDocument/hover", {**identifier, "position": position(self.text, at)})
        elif kind == "completion":
            at = anchor_offset(self.text, scenario["completion"], scenario, "completion")
            _, wall = self.server.request(
                "textDocument/completion", {**identifier, "position": position(self.text, at)}
            )
        elif kind == "inlay":
            _, wall = self.server.request(
                "textDocument/inlayHint",
                {**identifier, "range": {"start": {"line": 0, "character": 0}, "end": end_position(self.text)}},
            )
        else:
            _, wall = self.server.request("textDocument/semanticTokens/full", identifier)
        return wall


def measure_edit(server, document, scenario, apply):
    """Apply one edit and measure it: the keystroke-path requests first, then
    diagnostics, then idle, then the settled requests."""
    cpu_before = cpu_ms(server.pid)
    instructions_before = server.instructions.read()
    analyses_before = server.analyses_logged()
    started = time.perf_counter()
    apply()
    keystroke = {kind: document.ask(kind, scenario) * 1000 for kind in KEYSTROKE_REQUESTS}
    first = server.wait_publish(document.uri, started)
    # E244: a PAUSE row rests until the server has been quiet that long, so
    # whatever a real pause sets off (the dead-code clock's analyses) runs and
    # is counted; every other row resumes after 0.4 s of quiet.
    settled_at = server.settle(quiet_polls=int(scenario.get("pause", 0.4) * 10))
    last = server.last_publish(document.uri, started)
    cpu_after = cpu_ms(server.pid)
    instructions_after = server.instructions.read()
    analyses_after = server.analyses_logged()
    settled = {}
    for kind in KEYSTROKE_REQUESTS:
        before = live_threads_ns(server.pid)
        wall = document.ask(kind, scenario)
        settled[kind] = ((live_threads_ns(server.pid) - before) / 1e6, wall * 1000)
    errors = sum(1 for diagnostic in last[2] if diagnostic.get("severity", 1) == 1) if last else 0
    # Every publish the edit caused, to any file — the open dependents'
    # re-analyses among them.
    with server.publish_ready:
        caused = [entry for entry in server.publishes if started < entry[0] <= settled_at]
    everything = max(caused, key=lambda entry: entry[0]) if caused else None
    # The ANALYSIS's publish is the last one before idle: since E242 the
    # server republishes the followed diagnostics at once, before any
    # analysis, and that repaint is the first publish an edit gets.
    landed = last or first
    return {
        "diagnostics_ms": (landed[0] - started) * 1000,
        "diagnostics_cpu_ms": landed[3] - cpu_before,
        "first_publish_ms": (first[0] - started) * 1000,
        "instructions": (
            instructions_after - instructions_before
            if instructions_before is not None and instructions_after is not None
            else None
        ),
        "analyses": (
            analyses_after - (analyses_before or 0) if analyses_after is not None else None
        ),
        "all_published_ms": (everything[0] - started) * 1000 if everything else None,
        "all_published_cpu_ms": everything[3] - cpu_before if everything else None,
        "files_republished": len({entry[1] for entry in caused}),
        "cpu_ms": cpu_after - cpu_before,
        "settle_ms": (settled_at - started) * 1000,
        "errors": errors,
        "keystroke": keystroke,
        "settled": settled,
        "memory": memory_kb(server.pid),
    }


def measure_references(server, document, scenario):
    """One Find References at the scenario's anchor, at rest: CPU of the whole
    process (the request may build the entry's world), instructions, wall, and
    how many locations in how many files it answered."""
    at = anchor_offset(document.text, scenario["references"], scenario, "references")
    # Rest past the dead-code clock's idle window (600 ms) first, so its
    # entry analyses are not counted against the request.
    server.settle(quiet_polls=15)
    cpu_before = cpu_ms(server.pid)
    instructions_before = server.instructions.read()
    result, wall = server.request(
        "textDocument/references",
        {
            "textDocument": {"uri": document.uri},
            "position": position(document.text, at),
            "context": {"includeDeclaration": True},
        },
    )
    server.settle()
    instructions_after = server.instructions.read()
    locations = result or []
    return {
        "cpu_ms": cpu_ms(server.pid) - cpu_before,
        "wall_ms": wall * 1000,
        "instructions": (
            instructions_after - instructions_before
            if instructions_before is not None and instructions_after is not None
            else None
        ),
        "locations": len(locations),
        "files": len({location["uri"] for location in locations}),
    }


def run_scenario(server, root, scenario, runs, callgrind=False):
    companions = [Document(server, root / relative) for relative in scenario.get("also_open", [])]
    for companion in companions:
        opened = time.perf_counter()
        companion.open()
        server.wait_publish(companion.uri, opened)
        server.settle()
    document = Document(server, root / scenario["file"])
    opened = time.perf_counter()
    cpu_before = cpu_ms(server.pid)
    document.open()
    first = server.wait_publish(document.uri, opened)
    server.settle()
    cold = {
        "diagnostics_ms": (first[0] - opened) * 1000,
        "diagnostics_cpu_ms": first[3] - cpu_before,
        "cpu_ms": cpu_ms(server.pid) - cpu_before,
        "memory": memory_kb(server.pid),
    }
    rows = {scenario["name"]: []}
    if scenario.get("repair"):
        rows[scenario["repair"]] = []
    for _ in range(runs):
        at = anchor_offset(document.text, scenario["edit"], scenario, "edit")
        if callgrind:
            subprocess.run(["callgrind_control", "-i", "on", str(server.pid)], check=True, capture_output=True)
        rows[scenario["name"]].append(
            measure_edit(server, document, scenario, lambda: document.insert(at, scenario["text"]))
        )
        if callgrind:
            subprocess.run(["callgrind_control", "-i", "off", str(server.pid)], check=True, capture_output=True)
        undo = lambda: document.delete(at, len(scenario["text"]))
        if scenario.get("repair"):
            rows[scenario["repair"]].append(measure_edit(server, document, scenario, undo))
        else:
            undone = time.perf_counter()
            undo()
            server.wait_publish(document.uri, undone)
            server.settle()
    if scenario.get("references"):
        cold["references"] = [measure_references(server, document, scenario) for _ in range(2)]
    document.close()
    for samples in rows.values():
        for sample in samples:
            sample["companions"] = len(companions)
    for companion in companions:
        companion.close()
    server.settle()
    return cold, rows


def phase_fields(line, prefix):
    """`name value` pairs after `prefix`, as a dict of strings."""
    words = line[len(prefix):].split()
    return {words[index]: words[index + 1] for index in range(0, len(words) - 1, 2)}


def counter_fields(line, prefix):
    """`name=value` fields after `prefix`, as a dict of strings."""
    return dict(word.split("=", 1) for word in line[len(prefix):].split() if "=" in word)


def run_session(server, root, session):
    """M110 S0: the scripted session — per keystroke, what the analysis re-walked
    and what moved (the server's `hot-set` phase line and `incremental` counters
    line), its instructions and analyses, and Q10's broken-signature count."""
    companions = [Document(server, root / relative) for relative in SESSION_COMPANIONS]
    for companion in companions:
        opened = time.perf_counter()
        companion.open()
        server.wait_publish(companion.uri, opened)
        server.settle()
    rows = []
    for place in session:
        document = Document(server, root / place["file"])
        opened = time.perf_counter()
        document.open()
        server.wait_publish(document.uri, opened)
        server.settle()
        offset = anchor_offset(document.text, place["anchor"], {"name": "session", "file": place["file"]}, "anchor")
        typed = ""
        for number, keystroke in enumerate(place["keystrokes"], start=1):
            mark = server.stderr_mark()
            instructions_before = server.instructions.read()
            cpu_before = cpu_ms(server.pid)
            started = time.perf_counter()
            document.insert(offset, keystroke)
            offset += len(keystroke)
            typed += keystroke
            server.wait_publish(document.uri, started)
            server.settle()
            instructions_after = server.instructions.read()
            last = server.last_publish(document.uri, started)
            lines = server.stderr_since(mark)
            hot = [phase_fields(line, "[vilan phase]") for line in lines if line.startswith("[vilan phase] hot-set")]
            counters = [
                counter_fields(line, "[vilan counters] incremental")
                for line in lines
                if line.startswith("[vilan counters] incremental")
            ]
            moved = [line[len("[vilan phase] interface-moved-items "):] for line in lines
                     if line.startswith("[vilan phase] interface-moved-items")]
            verify = [line for line in lines if line.startswith("[vilan incremental] verify")]
            # The world analysis is the LAST one the keystroke ran (a further
            # platform leg or a dependent prints its own lines before it lands).
            hot_line = hot[-1] if hot else {}
            hot_set = hot_line.get("hot-set")
            counter = counters[-1] if counters else {}
            rows.append(
                {
                    "file": place["file"],
                    "keystroke": number,
                    "typed": typed,
                    "hot_set": hot_set,
                    "interface_moved": hot_line.get("interface-moved"),
                    "global_moved": hot_line.get("global-moved"),
                    "unknown_interfaces": hot_line.get("unknown-interfaces"),
                    "hot_world": hot_line.get("hot-world"),
                    "base_hits": counter.get("base-hits"),
                    "base_misses": counter.get("base-misses"),
                    "base_stores": counter.get("base-stores"),
                    "sources_walked": counter.get("sources-walked"),
                    "records_replayed": counter.get("records-replayed"),
                    "functions_checked": counter.get("functions-checked"),
                    "analyses": len(hot),
                    "instructions": (
                        instructions_after - instructions_before
                        if instructions_before is not None and instructions_after is not None
                        else None
                    ),
                    "cpu_ms": cpu_ms(server.pid) - cpu_before,
                    "errors": sum(1 for d in last[2] if d.get("severity", 1) == 1) if last else 0,
                    "moved": moved[-1] if moved else "",
                    "verify": [line.split()[3] for line in verify],
                }
            )
        undone = time.perf_counter()
        document.delete(offset - len(typed), len(typed))
        server.wait_publish(document.uri, undone)
        server.settle()
        document.close()
        server.settle()
    for companion in companions:
        companion.close()
    server.settle()
    return rows


def print_session(rows, header):
    print(header)
    print()
    print(
        "| file | # | typed so far | hot set | interface moved | global moved | unknown sigs | hot world "
        "| base hit/miss/store | sources walked | records replayed | functions checked | instructions:u (G) "
        "| analyses | errors | verify | moved items |"
    )
    print("|---|---:|---|---|---:|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|---|---|")
    for row in rows:
        instructions = "-" if row["instructions"] is None else f"{row['instructions'] / 1e9:.2f}"
        print(
            f"| {row['file']} | {row['keystroke']} | `{row['typed']}` | {row['hot_set']} "
            f"| {row['interface_moved']} | {row['global_moved']} | {row['unknown_interfaces']} | {row['hot_world']} "
            f"| {row['base_hits']}/{row['base_misses']}/{row['base_stores']} | {row['sources_walked']} "
            f"| {row['records_replayed']} | {row['functions_checked']} | {instructions} | {row['analyses']} "
            f"| {row['errors']} | {' '.join(row['verify']) or '-'} | {row['moved'][:160]} |"
        )
    print()
    print(
        "One row per keystroke, the client entry open beside the file (world mode). `hot set` is the edited "
        "module's reverse import closure, entry included, over the package modules the world reaches; "
        "`interface moved` counts the items whose interface fingerprint (signature with the inferred return, "
        "async, contexts, platform requirement, borrows/bumps) changed since the previous analysis; `global "
        "moved` is the impl/trait/resource facts; `unknown sigs` counts functions whose signature renders an "
        "unknown type (Q10). The counters are the world analysis's (the last one the keystroke ran)."
    )


def prepare_copy(kolt, commit, scratch):
    """A scratch copy of the package: `git archive` of `commit` when the source
    is a git checkout (reproducible), else the working tree minus build output."""
    kolt = Path(kolt).resolve()
    if (kolt / ".git").exists():
        sha = subprocess.run(
            ["git", "-C", str(kolt), "rev-parse", commit], check=True, capture_output=True, text=True
        ).stdout.strip()
        target = Path(scratch) / f"kolt-{sha[:8]}"
        if not target.exists():
            archive = subprocess.run(["git", "-C", str(kolt), "archive", sha], check=True, capture_output=True).stdout
            target.mkdir(parents=True)
            with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
                tar.extractall(target, filter="data")
        copy_untracked_inputs(kolt, target)
        return target, sha
    target = Path(scratch) / "kolt-worktree"
    if target.exists():
        shutil.rmtree(target)
    shutil.copytree(
        kolt, target, ignore=shutil.ignore_patterns("dist", "worktrees", "node_modules", ".git", "*.db", "target")
    )
    return target, "(working tree)"


def prepare_source(source, scratch):
    """`--source`: a prepared tree (across a breaking release, the migrated kolt), copied as it stands minus
    build output — no git is run in it, whatever it is."""
    source = Path(source).resolve()
    target = Path(scratch) / "kolt-source"
    if target.exists():
        shutil.rmtree(target)
    shutil.copytree(
        source, target, ignore=shutil.ignore_patterns("dist", "worktrees", "node_modules", ".git", "*.db", "target")
    )
    return target


def copy_untracked_inputs(kolt, target):
    """The gitignored GENERATED inputs — kolt's `src/search-dict/` (the const pass
    reads it) and `src/lucide/` (the icon module every view imports) — copied beside
    the archive when the checkout has them; without them the copy reports errors the
    owner's tree does not (56 `cannot find 'lucide'` at v0.46.0), and a server that
    analyzes a BROKEN program stops early, so every base row reads half its true cost
    and the seal's ratios inflate (Order 49's seal 1). `perf_gate.py prepare_kolt`
    carries the same two."""
    for relative in ("search-dict", "src/search-dict", "lucide", "src/lucide"):
        source = kolt / relative
        if source.is_dir() and not (target / relative).exists():
            shutil.copytree(source, target / relative)


def checkout_std_above(path):
    for directory in [path, *path.parents]:
        if (directory / "vilan" / "std" / "vilan.toml").is_file():
            return directory / "vilan" / "std"
    return None


def median(values):
    values = [value for value in values if value is not None]
    return statistics.median(values) if values else float("nan")


def print_table(cold_rows, rows, header):
    def counted(values, scale, digits):
        value = median(values)
        return "-" if value != value else f"{value / scale:.{digits}f}"

    print(header)
    print()
    print(
        "| edit | CPU to diagnostics ms | CPU to idle ms | instructions:u to idle (G) | analyses "
        "| wall to diagnostics ms | errors "
        "| keystroke wall ms (hover / completion / inlay / tokens) "
        "| idle CPU ms (hover / completion / inlay / tokens) | VmHWM MB |"
    )
    print("|---|---:|---:|---:|---:|---:|---:|---|---|---:|")
    for name, cold in cold_rows.items():
        print(
            f"| {name} | {cold['diagnostics_cpu_ms']:.0f} | {cold['cpu_ms']:.0f} | | | {cold['diagnostics_ms']:.0f} "
            f"| | | | {cold['memory']['VmHWM'] / 1024:.0f} |"
        )
    for name, samples in rows.items():
        keystroke = " / ".join(f"{median([s['keystroke'][k] for s in samples]):.1f}" for k in KEYSTROKE_REQUESTS)
        settled = " / ".join(f"{median([s['settled'][k][0] for s in samples]):.2f}" for k in KEYSTROKE_REQUESTS)
        print(
            f"| {name} | {median([s['diagnostics_cpu_ms'] for s in samples]):.0f} "
            f"| {median([s['cpu_ms'] for s in samples]):.0f} "
            f"| {counted([s.get('instructions') for s in samples], 1e9, 2)} "
            f"| {counted([s.get('analyses') for s in samples], 1, 0)} "
            f"| {median([s['diagnostics_ms'] for s in samples]):.0f} "
            f"| {samples[-1]['errors']} | {keystroke} | {settled} "
            f"| {max(s['memory']['VmHWM'] for s in samples) / 1024:.0f} |"
        )
    for name, cold in cold_rows.items():
        for label, sample in zip(("first", "warm"), cold.get("references", [])):
            instructions = sample["instructions"]
            print(
                f"| {name}: Find References ({label}) | {sample['cpu_ms']:.0f} | | "
                f"{'-' if instructions is None else f'{instructions / 1e9:.2f}'} | | {sample['wall_ms']:.0f} "
                f"| {sample['locations']} locations in {sample['files']} files | | | |"
            )
    for name, samples in rows.items():
        if any(s.get("companions") for s in samples):
            print(
                f"| {name}: every open file | {median([s['all_published_cpu_ms'] for s in samples]):.0f} "
                f"| | | | {median([s['all_published_ms'] for s in samples]):.0f} "
                f"| {samples[-1]['files_republished']} files | | | |"
            )
    print()
    print("Medians over the runs. E121's targets: <10 ms on the keystroke path, <500 ms to errors.")
    print(
        "CPU is the whole server process (every thread) from the edit to the analysis's publish for the file, and "
        "to idle; instructions are user-space, every thread, edit to idle; wall includes the debounce and the "
        "machine's load. Keystroke requests are asked before the analysis lands; idle requests after it. A pause "
        "row rests until the server has been quiet for its pause; every other row resumes after 0.4 s."
    )


def callgrind_report(directory, top):
    outputs = sorted(Path(directory).glob("callgrind.out.*"), key=lambda path: path.stat().st_size)
    if not outputs:
        print("callgrind wrote no profile", file=sys.stderr)
        return
    annotated = subprocess.run(
        ["callgrind_annotate", "--inclusive=no", str(outputs[-1])], capture_output=True, text=True
    ).stdout
    lines = annotated.splitlines()
    start = next((index for index, line in enumerate(lines) if "file:function" in line), None)
    print(f"\ncallgrind (self cost, Ir) — {outputs[-1]}")
    for line in lines[: (start or 0) + top + 2] if start is not None else lines[: top + 20]:
        print(line)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sources = parser.add_mutually_exclusive_group(required=True)
    sources.add_argument("--kolt", help="the package checkout to copy (kolt), at --commit")
    sources.add_argument(
        "--source",
        help="a PREPARED tree to replay the script over, used as it stands (no git): the tip's migrated kolt "
        "across a breaking release",
    )
    parser.add_argument("--commit", default="HEAD", help="the commit to copy (git checkouts; default HEAD)")
    parser.add_argument("--lsp", default="vilan-lsp", help="the vilan-lsp binary (default: the one on PATH)")
    parser.add_argument("--scratch", default=None, help="where the copy goes (default: a temp dir)")
    parser.add_argument("--std", default=None, help="VILAN_STD for the server (default: the server's own discovery)")
    parser.add_argument("--runs", type=int, default=5, help="repetitions per edit (default 5)")
    parser.add_argument("--scenario", action="append", help="run only these scenarios (by name)")
    parser.add_argument("--json", default=None, help="also write every sample here")
    parser.add_argument("--callgrind", default=None, help="profile one scenario's edit into this directory")
    parser.add_argument("--top", type=int, default=25, help="callgrind offenders to print (default 25)")
    parser.add_argument(
        "--session",
        action="store_true",
        help="M110 S0: run the scripted keystroke session (ten keystrokes in views.vl, theme.vl, model.vl and a "
        "css block, world mode) instead of the latency scenarios, and print what each keystroke re-walked and "
        "moved; the server runs with VILAN_COUNTERS=1 and VILAN_INCREMENTAL=measure unless already set",
    )
    arguments = parser.parse_args()

    scratch = Path(arguments.scratch or os.path.join(os.environ.get("TMPDIR", "/tmp"), "vilan-lsp-latency"))
    if arguments.source:
        root = prepare_source(arguments.source, scratch)
        source = {"kind": "prepared", "path": str(Path(arguments.source).resolve())}
        described = f"the prepared tree {source['path']}"
    else:
        root, sha = prepare_copy(arguments.kolt, arguments.commit, scratch)
        source = {"kind": "commit", "sha": sha}
        described = f"kolt @{sha[:8]}"
    lsp = shutil.which(arguments.lsp) or arguments.lsp
    # The server runs in the copy, so a relative binary path is resolved here,
    # first (`--lsp target/release/vilan-lsp` named nothing from inside it).
    lsp = os.path.abspath(lsp) if os.sep in lsp else lsp
    version = subprocess.run([lsp, "--version"], capture_output=True, text=True).stdout.strip()
    env = dict(os.environ)
    if arguments.std:
        env["VILAN_STD"] = str(Path(arguments.std).resolve())
    discovered = checkout_std_above(root)
    std_note = (
        f"VILAN_STD={env['VILAN_STD']}"
        if "VILAN_STD" in env
        else (f"std discovered at {discovered} (an ancestor checkout)" if discovered else "the server's embedded std")
    )
    if arguments.session:
        return run_session_main(arguments, root, described, lsp, version, env, scratch, std_note)
    scenarios = [s for s in SCENARIOS if not arguments.scenario or s["name"] in arguments.scenario]
    if not scenarios:
        raise SystemExit(f"no scenario named {arguments.scenario}; the names: {[s['name'] for s in SCENARIOS]}")
    problems = anchor_problems(root, scenarios)
    if problems:
        raise SystemExit(
            f"the edit script does not land in {described}:\n  "
            + "\n  ".join(problems)
            + "\nNothing was run. Pass the commit the script names, or leave the scenario out with --scenario."
        )

    command = [lsp]
    if arguments.callgrind:
        Path(arguments.callgrind).mkdir(parents=True, exist_ok=True)
        out = Path(arguments.callgrind).resolve() / "callgrind.out.%p"
        command = ["valgrind", "--tool=callgrind", "--instr-atstart=no", f"--callgrind-out-file={out}", lsp]
        scenarios = scenarios[:1]
        arguments.runs = 1

    # The server prints one `lsp-context` line per analysis under this
    # variable — the analysis count per edit (E244). It writes to stderr only.
    env.setdefault("VILAN_PHASE_TIMING", "1")
    load_before = load_average()
    server = Server(command, root, env, scratch / "vilan-lsp.stderr")
    print(f"server pid {server.pid}: {' '.join(command)}", file=sys.stderr)
    try:
        server.request(
            "initialize",
            {
                "processId": os.getpid(),
                "rootUri": root.resolve().as_uri(),
                "capabilities": {
                    "textDocument": {
                        "publishDiagnostics": {},
                        "hover": {"contentFormat": ["markdown", "plaintext"]},
                        "completion": {"completionItem": {"snippetSupport": True}},
                        "inlayHint": {},
                        "semanticTokens": {"requests": {"full": True}, "formats": ["relative"], "tokenTypes": [], "tokenModifiers": []},
                    },
                    "workspace": {"inlayHint": {"refreshSupport": True}, "semanticTokens": {"refreshSupport": True}},
                },
            },
        )
        server.notify("initialized", {})
        # E244: attached once the server's threads exist; threads it spawns
        # later inherit the counter.
        server.instructions = InstructionCounter(server.pid)
        cold_rows, rows = {}, {}
        for scenario in scenarios:
            print(f"  {scenario['name']} ...", file=sys.stderr)
            cold, scenario_rows = run_scenario(
                server, root, scenario, arguments.runs, callgrind=bool(arguments.callgrind)
            )
            companions = len(scenario.get("also_open", []))
            suffix = f" (beside {companions} open importers)" if companions else ""
            cold_rows[f"open {scenario['file']}{suffix}"] = cold
            rows.update(scenario_rows)
        if arguments.callgrind:
            subprocess.run(["callgrind_control", "-d", str(server.pid)], capture_output=True)
    finally:
        server.stop()
    header = (
        f"vilan-lsp latency — {version} on {described} ({std_note}); "
        f"{arguments.runs} run(s) per edit; loadavg {load_before} before, {load_average()} after; "
        f"{os.cpu_count()} cores"
    )
    print_table(cold_rows, rows, header)
    if arguments.json:
        Path(arguments.json).write_text(
            json.dumps({"header": header, "source": source, "open": cold_rows, "edits": rows}, indent=1)
        )
    if arguments.callgrind:
        callgrind_report(arguments.callgrind, arguments.top)


def start_server(command, root, env, scratch):
    server = Server(command, root, env, scratch / "vilan-lsp.stderr")
    print(f"server pid {server.pid}: {' '.join(command)}", file=sys.stderr)
    server.request(
        "initialize",
        {
            "processId": os.getpid(),
            "rootUri": root.resolve().as_uri(),
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": {},
                    "hover": {"contentFormat": ["markdown", "plaintext"]},
                    "completion": {"completionItem": {"snippetSupport": True}},
                    "inlayHint": {},
                    "semanticTokens": {"requests": {"full": True}, "formats": ["relative"], "tokenTypes": [], "tokenModifiers": []},
                },
                "workspace": {"inlayHint": {"refreshSupport": True}, "semanticTokens": {"refreshSupport": True}},
            },
        },
    )
    server.notify("initialized", {})
    server.instructions = InstructionCounter(server.pid)
    return server


def run_session_main(arguments, root, described, lsp, version, env, scratch, std_note):
    problems = []
    for place in SESSION:
        text = (root / place["file"]).read_text()
        count = text.count(place["anchor"][0])
        if count != 1:
            problems.append(f"{place['file']}: the session anchor {place['anchor'][0]!r} occurs {count} times")
    if problems:
        raise SystemExit("the session does not land in " + described + ":\n  " + "\n  ".join(problems))
    env.setdefault("VILAN_PHASE_TIMING", "1")
    env.setdefault("VILAN_COUNTERS", "1")
    env.setdefault("VILAN_INCREMENTAL", "measure")
    load_before = load_average()
    server = start_server([lsp], root, env, scratch)
    try:
        rows = run_session(server, root, SESSION)
    finally:
        server.stop()
    header = (
        f"vilan-lsp keystroke session (M110 S0) — {version} on {described} ({std_note}); "
        f"VILAN_INCREMENTAL={env['VILAN_INCREMENTAL']}; loadavg {load_before} before, {load_average()} after"
    )
    print_session(rows, header)
    if arguments.json:
        Path(arguments.json).write_text(json.dumps({"header": header, "session": rows}, indent=1))


if __name__ == "__main__":
    main()
