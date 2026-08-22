#!/bin/sh
# The tier-2 memory-leak soak (proposal/leak-soak.md §3): the toolchain's two
# LONG-LIVED processes, driven for minutes and sampled the whole way.
#
#   scripts/soak.sh                                # both legs, the defaults
#   scripts/soak.sh --rounds 60 --requests 20000   # a longer run
#   scripts/soak.sh --leg watch                    # one leg only
#   scripts/soak.sh --help                         # every option
#
# Two legs, because the toolchain has exactly two processes a person leaves
# running for hours:
#
#   watch   `vilan run --watch` on a two-leg fullstack fixture, through N
#           rebuild rounds, with SSE browsers connecting and disconnecting
#           between every round. That churn is what backlog M3's file-descriptor
#           leak lived in (hmr.md's M3 appendix, fixed 2026-08-18), so this leg
#           is that fix's field validation: descriptors, threads and RSS are
#           read from /proc/<watcher>/ three times a round — idle, with the
#           browsers connected, and after they leave — and must return to the
#           same idle figure every round.
#   server  the compiled Node server itself, run directly under `node`, under M
#           sustained requests split between the page route and the rpc route.
#           Its RSS is sampled per batch. What this leg produces is a CURVE, not
#           a verdict against a threshold, and note what it measures: vilan's
#           standard library as it runs in JavaScript, on V8's heap, with V8's
#           collector deciding when to give memory back — outside Rust's memory
#           model entirely, and outside anything `leak_tally` can see.
#
# The LSP edit storm is deliberately NOT a leg here. No JSON-RPC protocol
# harness exists in this repository to drive a real `vilan-lsp` process over
# stdio, and inventing one for a soak would be a larger and less trustworthy
# instrument than the one that already exists: tier 1 drives `Document::analyze`
# — the exact entry point the server's `spawn_blocking` wraps — for thousands of
# keystrokes and reads EXACT per-site leak counters, where a protocol harness
# could only have watched RSS. Its command is in §5 of the paper, beside this
# script's.
#
# Three house rules, each of them scar tissue:
#
#  - **Every fixture self-expires.** Each fixture server sleeps out a deadline
#    and exits on its own, so a soak whose driver is killed leaves nothing
#    running. The deadline is derived from the configured run, never guessed.
#  - **Every process is killed AND asserted dead.** SIGKILLing the watcher does
#    not reap its Node grandchild (E60, the css e2e's lesson), so every fixture
#    server carries a `/shutdown` route and its death is witnessed by a refused
#    connection — not assumed from a kill that returned.
#  - **The zombie sweep matches the process NAME** (`pgrep -x node`), never
#    `pgrep -f`, which matches this script's own command line and reports itself
#    as the leak it was looking for.
#
# Nothing here asserts a threshold. Every leg prints a table; the paper's §4
# dispositions what the tables show. Exit status is 0 when the soak RAN — a
# fixture that would not build, a server that would not come up, or a process
# that would not die are the failures, because each of them means there is no
# measurement.
set -eu

REPO=$(cd "$(dirname "$0")/.." && pwd)

ROUNDS=20
REQUESTS=4000
CLIENTS=4
BATCH=250
SETTLE=10
HEAP_CAP=""
LEG=both
KEEP=0
VILAN=${VILAN_BIN:-}

usage() {
    sed -n '2,55p' "$0" | sed 's/^# \{0,1\}//'
    cat <<'USAGE'

Options:
  --rounds N      watch rebuild rounds (default 20)
  --clients N     SSE browsers connected and dropped between rounds (default 4)
  --requests N    requests against the compiled server (default 4000)
  --batch N       requests per RSS sample (default 250)
  --settle N      seconds idle before the final sample of each leg (default 10)
  --heap-cap MiB  run node under --max-old-space-size=MiB. Not tuning: it is
                  the cheap discriminator between a curve that is V8 growing
                  its heap because nothing made it collect, and one that is
                  unbounded retention. Bounded retention survives a small cap;
                  a real leak dies on it with a heap-out-of-memory abort.
  --leg WHICH     watch | server | both (default both)
  --bin PATH      the `vilan` binary (default: target/release, else target/debug)
  --keep          keep the work directory instead of deleting it
USAGE
}

while [ $# -gt 0 ]; do
    case "$1" in
        --rounds) ROUNDS=${2:?--rounds needs a count}; shift 2 ;;
        --clients) CLIENTS=${2:?--clients needs a count}; shift 2 ;;
        --requests) REQUESTS=${2:?--requests needs a count}; shift 2 ;;
        --batch) BATCH=${2:?--batch needs a count}; shift 2 ;;
        --settle) SETTLE=${2:?--settle needs seconds}; shift 2 ;;
        --heap-cap) HEAP_CAP=${2:?--heap-cap needs MiB}; shift 2 ;;
        --leg) LEG=${2:?--leg needs watch|server|both}; shift 2 ;;
        --bin) VILAN=${2:?--bin needs a path}; shift 2 ;;
        --keep) KEEP=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'soak: unknown option %s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
done

case "$LEG" in
    watch|server|both) ;;
    *) printf 'soak: --leg must be watch, server or both\n' >&2; exit 2 ;;
esac

if [ -z "$VILAN" ]; then
    if [ -x "$REPO/target/release/vilan" ]; then
        VILAN="$REPO/target/release/vilan"
    elif [ -x "$REPO/target/debug/vilan" ]; then
        VILAN="$REPO/target/debug/vilan"
    else
        printf 'soak: no vilan binary; run `cargo build --release` or pass --bin\n' >&2
        exit 2
    fi
fi
[ -x "$VILAN" ] || { printf 'soak: %s is not executable\n' "$VILAN" >&2; exit 2; }
command -v node > /dev/null 2>&1 || { printf 'soak: node is not on PATH\n' >&2; exit 2; }
command -v curl > /dev/null 2>&1 || { printf 'soak: curl is not on PATH\n' >&2; exit 2; }
[ -d /proc/self ] || { printf 'soak: /proc is required (this soak is Linux-only)\n' >&2; exit 2; }

# The fixtures' self-expiry. Generous against the configured run — a round is a
# compile, and a compile on a loaded box is not a number this script should be
# guessing at — but finite, which is the whole job: nothing survives it.
LIFE_SECONDS=$(( 600 + ROUNDS * 60 ))
LIFE_MS=$(( LIFE_SECONDS * 1000 ))
# How long one bounded wait may take. A liveness bound, never a performance
# assertion: `support::WATCH_LIVENESS`'s 300 s, for its recorded reason (a first
# watch round is a full compile of every leg, and on a contended box that alone
# runs past a minute).
LIVENESS=300

WORK=$(mktemp -d "${TMPDIR:-/tmp}/vilan-soak-XXXXXX")
WATCH_DIR="$WORK/watch"
SERVER_DIR="$WORK/server"
WATCH_LOG="$WORK/watch.log"
SERVER_LOG="$WORK/server.log"

WATCHER_PID=""
WATCHER_SERVER_PORT=""
SERVER_PID=""
SERVER_PORT=""
SSE_PIDS=""
DEV_PORT=""
ZOMBIES_BEFORE=""

say() { printf '%s\n' "$*"; }
note() { printf 'soak: %s\n' "$*"; }

# --- process helpers ---------------------------------------------------------

# Every live `node` by process NAME. `-x` matches `comm`, so this can never see
# the soak's own command line the way `pgrep -f node` would.
node_pids() { pgrep -x node 2>/dev/null | sort -n | tr '\n' ' '; }

proc_fds() { ls "/proc/$1/fd" 2>/dev/null | wc -l | tr -d ' '; }
proc_field() { awk -v key="$2:" '$1 == key { print $2; exit }' "/proc/$1/status" 2>/dev/null; }
proc_threads() { proc_field "$1" Threads; }
proc_rss_kib() { proc_field "$1" VmRSS; }
alive() { [ -n "$1" ] && kill -0 "$1" 2>/dev/null; }

# Bounded wait for `needle` to appear in `file`. Returns 1 at the deadline
# rather than hanging: a soak that stops making progress must say so.
wait_for_line() { # file needle seconds label
    waited=0
    while [ "$waited" -lt "$3" ]; do
        if [ -f "$1" ] && grep -q "$2" "$1" 2>/dev/null; then
            return 0
        fi
        sleep 1
        waited=$(( waited + 1 ))
    done
    note "timed out after ${3}s waiting for $4"
    return 1
}

# The CLI paints its dev-channel line, so read the log ANSI-free.
plain_log() { sed -e 's/\x1b\[[0-9;]*m//g' "$1" 2>/dev/null; }

# Asks the fixture server on `port` to exit and WITNESSES that it did: each poll
# re-sends `/shutdown` (a request that lands exits within milliseconds) and a
# refused connection is the death certificate. This is the E60 pin — killing the
# watcher orphans its node grandchild, so the grandchild has to die by its own
# route and be seen to.
assert_dead_on_shutdown() { # port label
    [ -n "$1" ] || return 0
    waited=0
    while [ "$waited" -lt 30 ]; do
        if ! curl -s -m 2 -o /dev/null "http://127.0.0.1:$1/shutdown" 2>/dev/null; then
            say "  $2 on port $1: dead (connection refused)"
            return 0
        fi
        sleep 1
        waited=$(( waited + 1 ))
    done
    note "FAILED: $2 on port $1 is still answering after /shutdown — an orphan"
    return 1
}

drop_sse_clients() {
    [ -n "$SSE_PIDS" ] || return 0
    for pid in $SSE_PIDS; do kill "$pid" 2>/dev/null || true; done
    for pid in $SSE_PIDS; do wait "$pid" 2>/dev/null || true; done
    SSE_PIDS=""
}

cleanup() {
    status=$?
    trap - EXIT INT TERM
    drop_sse_clients
    if alive "$WATCHER_PID"; then kill "$WATCHER_PID" 2>/dev/null || true; wait "$WATCHER_PID" 2>/dev/null || true; fi
    # The watcher's temp round script is keyed by its pid and outlives a SIGKILL
    # (the Ctrl-C hook never runs) — the same harness-only cleanup
    # `support::kill_watcher` does.
    [ -n "$WATCHER_PID" ] && rm -f "${TMPDIR:-/tmp}/vilan-watch-$WATCHER_PID.mjs"
    assert_dead_on_shutdown "$WATCHER_SERVER_PORT" "the watch fixture's server" || status=1
    if [ -n "$SERVER_PORT" ]; then
        assert_dead_on_shutdown "$SERVER_PORT" "the compiled server" || status=1
    fi
    if alive "$SERVER_PID"; then kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; fi

    say ""
    say "== zombie sweep (pgrep -x node) =="
    after=$(node_pids)
    say "  before: ${ZOMBIES_BEFORE:-<none>}"
    say "  after:  ${after:-<none>}"
    leftover=""
    for pid in $after; do
        case " $ZOMBIES_BEFORE " in
            *" $pid "*) ;;
            *) leftover="$leftover $pid" ;;
        esac
    done
    if [ -n "$leftover" ]; then
        note "FAILED: node processes survived the soak:$leftover"
        ps -o pid=,etime=,args= -p "$(echo "$leftover" | tr ' ' ',' | sed 's/^,//')" 2>/dev/null || true
        status=1
    else
        say "  no node process outlived the soak"
    fi

    if [ "$KEEP" -eq 1 ]; then
        say "  work directory kept: $WORK"
    else
        rm -rf "$WORK"
    fi
    exit "$status"
}
trap cleanup EXIT INT TERM

# --- fixtures ----------------------------------------------------------------

# The browser leg both fixtures carry: one `const` initializer emitting a
# stylesheet, so `dist/` gets a css sidecar and the workspace is HMR-eligible
# (a `run --watch` stands up its dev channel only for a workspace with a browser
# leg). Copied in shape from `tests/hmr.rs`'s own client fixture.
write_client() { # dir
    mkdir -p "$1/src"
    cat > "$1/src/client.vl" <<'CLIENT'
import std::print;
import std::asset::emit;

fun styles(): i32 {
	emit("css", ".soak{color:red}");
	1
}

let _s = const styles();

fun main() {
	print("soak client");
}
CLIENT
}

# The watch leg's server: it deliberately OUTLIVES a round (that is what makes
# the fd/thread accounting interesting), announces the round it belongs to so a
# rebuild is witnessed rather than assumed, answers `/shutdown`, and expires on
# its own.
write_watch_server() { # dir round
    cat > "$1/src/server.vl" <<SERVER
import std::http::{ Response, Server };
import std::print;
import std::process;
import std::time::sleep;

async fun main() {
	Server::builder()
		.port(0)
		.on_request(|request| {
			match request.path() {
				"/shutdown" => {
					process::exit(0);
					Response::builder().body("").build()
				},
				_ => Response::builder().body("soak round $2").build(),
			}
		})
		.on_start(|server| print(i"soak-server-up $2 {server.port()}"))
		.build()
		.start();
	sleep($LIFE_MS);
	process::exit(0);
}
SERVER
}

write_watch_fixture() {
    mkdir -p "$WATCH_DIR/src"
    cat > "$WATCH_DIR/vilan.toml" <<'MANIFEST'
[package]
name = "soak_watch"

[entry.client]
target = "browser"

[entry.server]
MANIFEST
    write_client "$WATCH_DIR"
    write_watch_server "$WATCH_DIR" 1
}

# The server leg's fixture: the todo app's shape — a browser leg, a page built
# from its artifacts, and an rpc service mounted at `/` — small enough to be
# read at a glance and real enough that `/rpc` runs a genuine dispatch through
# the wire codec and a reactive turn.
write_server_fixture() {
    mkdir -p "$SERVER_DIR/src"
    cat > "$SERVER_DIR/vilan.toml" <<'MANIFEST'
[package]
name = "soak_app"

[entry.client]
target = "browser"

[entry.server]
MANIFEST
    write_client "$SERVER_DIR"
    cat > "$SERVER_DIR/src/server.vl" <<SERVER
import std::build::require_build;
import std::document::Document;
import std::http::{ Response, Server };
import std::json::json_codec;
import std::print;
import std::process;
import std::reactive::Signal;
import std::rpc_server::Service;
import std::shared::Shared;
import std::time::sleep;

[service(SoakClient)]
struct Counter {
	[expose] total: Signal<i32>,
	calls: Shared<i32>,
}

impl Counter {
	fun new(): Counter {
		Counter { total = Signal::new(0), calls = Shared::new(0) }
	}

	[rpc]
	fun add(self, by: i32): i32 {
		self.calls.write() += 1;
		self.total.set(self.total.get() + by);
		self.total.get()
	}
}

async fun main() {
	let build = require_build("client");
	let page = Document::of(build).title("Soak").html();
	let counter = Counter::new();

	Server::builder()
		.port(0)
		.with_service(Service::new(counter.dispatcher().into_protocol(json_codec())))
		.serve_build(build)
		.on_request(|request| {
			match request.path() {
				"/shutdown" => {
					process::exit(0);
					Response::builder().body("").build()
				},
				_ => Response::builder().set_header("Content-Type", "text/html").body(page).build(),
			}
		})
		.on_start(|server| print(i"soak-app-up {server.port()}"))
		.build()
		.start();
	sleep($LIFE_MS);
	process::exit(0);
}
SERVER
}

# --- leg: the watch session --------------------------------------------------

# Opens `CLIENTS` SSE connections to the dev channel and leaves them open. Raw
# `curl -N` against `/events`: the channel writes a head and a `connected` hello
# and then parks, which is exactly a browser tab.
open_sse_clients() {
    SSE_PIDS=""
    n=0
    while [ "$n" -lt "$CLIENTS" ]; do
        curl -sN -m "$LIVENESS" "http://127.0.0.1:$DEV_PORT/events" > /dev/null 2>&1 &
        SSE_PIDS="$SSE_PIDS $!"
        n=$(( n + 1 ))
    done
    # One second, so every connection has reached the registry before it is
    # counted. The measurement below is what proves it: with the browsers open
    # the descriptor count RISES, which is how "it came back down" means
    # anything at all.
    sleep 1
}

run_watch_leg() {
    say ""
    say "== leg 1: \`vilan run --watch\` under $ROUNDS rounds and $CLIENTS-browser churn =="
    write_watch_fixture
    "$VILAN" run --watch --hmr-port 0 "$WATCH_DIR" > "$WATCH_LOG" 2>&1 &
    WATCHER_PID=$!
    say "  watcher pid $WATCHER_PID, fixture $WATCH_DIR"

    wait_for_line "$WATCH_LOG" 'hmr: dev channel on 127.0.0.1:' "$LIVENESS" "the dev channel" || return 1
    DEV_PORT=$(plain_log "$WATCH_LOG" | sed -n 's/^hmr: dev channel on 127\.0\.0\.1:\([0-9]*\).*/\1/p' | head -1)
    [ -n "$DEV_PORT" ] || { note "could not read the dev-channel port"; return 1; }
    say "  dev channel on 127.0.0.1:$DEV_PORT"

    say ""
    say "  round | fds idle | fds open | fds after | thr idle | thr open | thr after | rss KiB"
    say "  ------+----------+----------+-----------+----------+----------+-----------+--------"

    round=1
    while [ "$round" -le "$ROUNDS" ]; do
        if [ "$round" -gt 1 ]; then
            write_watch_server "$WATCH_DIR" "$round"
        fi
        wait_for_line "$WATCH_LOG" "soak-server-up $round " "$LIVENESS" "round $round to rebuild and restart its server" || return 1
        WATCHER_SERVER_PORT=$(plain_log "$WATCH_LOG" | sed -n "s/^soak-server-up $round \([0-9]*\).*/\1/p" | head -1)

        fds_idle=$(proc_fds "$WATCHER_PID")
        threads_idle=$(proc_threads "$WATCHER_PID")
        open_sse_clients
        fds_open=$(proc_fds "$WATCHER_PID")
        threads_open=$(proc_threads "$WATCHER_PID")
        drop_sse_clients
        # The disconnect is asynchronous on the server's side (the connection's
        # own reader wakes on end-of-stream and unregisters), so give it the
        # same second the connect got before reading the count back.
        sleep 1
        fds_after=$(proc_fds "$WATCHER_PID")
        threads_after=$(proc_threads "$WATCHER_PID")
        rss=$(proc_rss_kib "$WATCHER_PID")

        printf '  %5d | %8s | %8s | %9s | %8s | %8s | %9s | %7s\n' \
            "$round" "$fds_idle" "$fds_open" "$fds_after" \
            "$threads_idle" "$threads_open" "$threads_after" "$rss"
        printf 'SOAK {"leg":"watch","round":%d,"fds_idle":%s,"fds_open":%s,"fds_after":%s,"threads_idle":%s,"threads_open":%s,"threads_after":%s,"rss_kib":%s}\n' \
            "$round" "$fds_idle" "$fds_open" "$fds_after" \
            "$threads_idle" "$threads_open" "$threads_after" "$rss" >> "$WORK/soak.jsonl"
        round=$(( round + 1 ))
    done
    if [ "$SETTLE" -gt 0 ]; then
        sleep "$SETTLE"
        printf '  %5s | %8s | %8s | %9s | %8s | %8s | %9s | %7s\n' \
            "idle" "$(proc_fds "$WATCHER_PID")" "-" "-" \
            "$(proc_threads "$WATCHER_PID")" "-" "-" "$(proc_rss_kib "$WATCHER_PID")"
    fi
    say ""
    say "  rows also written to $WORK/soak.jsonl"
}

# --- leg: the compiled Node server -------------------------------------------

# `count` copies of `url`, as an argument list. curl reuses one connection
# across the URLs it is handed, which is what a browser does and what keeps this
# loop measuring the SERVER rather than process startup.
repeat_url() { # count url
    n=0
    out=""
    while [ "$n" -lt "$1" ]; do
        out="$out $2"
        n=$(( n + 1 ))
    done
    printf '%s' "$out"
}

run_server_leg() {
    say ""
    say "== leg 2: the compiled Node server under $REQUESTS requests =="
    write_server_fixture
    if ! "$VILAN" build "$SERVER_DIR" > "$WORK/build.log" 2>&1; then
        note "the server fixture did not build:"
        cat "$WORK/build.log" >&2
        return 1
    fi
    if [ -n "$HEAP_CAP" ]; then
        ( cd "$SERVER_DIR" && exec node "--max-old-space-size=$HEAP_CAP" dist/server.mjs ) > "$SERVER_LOG" 2>&1 &
    else
        ( cd "$SERVER_DIR" && exec node dist/server.mjs ) > "$SERVER_LOG" 2>&1 &
    fi
    SERVER_PID=$!
    wait_for_line "$SERVER_LOG" 'soak-app-up ' "$LIVENESS" "the compiled server to bind" || return 1
    SERVER_PORT=$(sed -n 's/^soak-app-up \([0-9]*\).*/\1/p' "$SERVER_LOG" | head -1)
    [ -n "$SERVER_PORT" ] || { note "could not read the server's port"; return 1; }
    BASE="http://127.0.0.1:$SERVER_PORT"
    say "  node pid $SERVER_PID on 127.0.0.1:$SERVER_PORT"

    page_code=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/")
    rpc_body=$(curl -s -H 'Content-Type: application/json' -d '{"method":"add","args":[1]}' "$BASE/rpc")
    say "  sanity: GET / -> $page_code, POST /rpc -> $rpc_body"
    case "$page_code" in 200) ;; *) note "the page route did not answer 200"; return 1 ;; esac

    say ""
    say "  requests | rss KiB | fds | threads"
    say "  ---------+---------+-----+--------"
    printf '  %8d | %7s | %3s | %7s\n' 0 "$(proc_rss_kib "$SERVER_PID")" \
        "$(proc_fds "$SERVER_PID")" "$(proc_threads "$SERVER_PID")"

    done_count=0
    half=$(( BATCH / 2 ))
    [ "$half" -ge 1 ] || half=1
    while [ "$done_count" -lt "$REQUESTS" ]; do
        # Redirect the whole invocation rather than passing `-o`: curl binds
        # one `-o` to one URL, so `-o /dev/null` with fifty URLs discards the
        # first body and prints the other forty-nine.
        # shellcheck disable=SC2046  # the repeated URL is one word by construction
        curl -s $(repeat_url "$half" "$BASE/") > /dev/null 2>&1 || true
        # shellcheck disable=SC2046
        curl -s -H 'Content-Type: application/json' \
            -d '{"method":"add","args":[1]}' $(repeat_url "$half" "$BASE/rpc") \
            > /dev/null 2>&1 || true
        done_count=$(( done_count + half * 2 ))
        rss=$(proc_rss_kib "$SERVER_PID")
        fds=$(proc_fds "$SERVER_PID")
        threads=$(proc_threads "$SERVER_PID")
        printf '  %8d | %7s | %3s | %7s\n' "$done_count" "$rss" "$fds" "$threads"
        printf 'SOAK {"leg":"server","requests":%d,"rss_kib":%s,"fds":%s,"threads":%s}\n' \
            "$done_count" "$rss" "$fds" "$threads" >> "$WORK/soak.jsonl"
        alive "$SERVER_PID" || { note "the server died mid-soak; see $SERVER_LOG"; return 1; }
    done
    # The settle sample. A rising RSS curve under load is not by itself a
    # leak: V8 grows its heap while nothing forces it to collect. What
    # separates "grew" from "retains" is what the number does once the load
    # stops, so the last row is read after an idle window rather than at the
    # peak.
    if [ "$SETTLE" -gt 0 ]; then
        sleep "$SETTLE"
        rss=$(proc_rss_kib "$SERVER_PID")
        fds=$(proc_fds "$SERVER_PID")
        threads=$(proc_threads "$SERVER_PID")
        printf '  %8s | %7s | %3s | %7s   (after %ss idle)\n' "settled" "$rss" "$fds" "$threads" "$SETTLE"
        printf 'SOAK {"leg":"server","requests":%d,"settled_after_s":%s,"rss_kib":%s,"fds":%s,"threads":%s}\n' \
            "$done_count" "$SETTLE" "$rss" "$fds" "$threads" >> "$WORK/soak.jsonl"
    fi
    say ""
    say "  final rpc total: $(curl -s -H 'Content-Type: application/json' -d '{"method":"add","args":[0]}' "$BASE/rpc")"
}

# --- run ---------------------------------------------------------------------

ZOMBIES_BEFORE=$(node_pids)
say "vilan leak soak — $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
say "  binary:   $VILAN ($("$VILAN" --version 2>/dev/null || echo '?'))"
say "  work:     $WORK"
say "  fixtures self-expire after ${LIFE_SECONDS}s"
say "  node before: ${ZOMBIES_BEFORE:-<none>}"

case "$LEG" in
    watch) run_watch_leg ;;
    server) run_server_leg ;;
    both) run_watch_leg && run_server_leg ;;
esac
