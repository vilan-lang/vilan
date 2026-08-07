//! Pins the benchmark HARNESS (vilan/benchmarks): it must build, run to
//! completion, and report the deterministic facts — the coalescing and
//! fan-out frame counts are exact invariants of the reactive protocol.
//! Timings are machine-dependent and deliberately not asserted.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod support;

#[test]
fn benchmarks_run_and_report_the_deterministic_counts() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/benchmarks");
    let mut child = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn vilan run");
    // A liveness bound, and nothing more: this file asserts counts, not speed
    // ("Timings are machine-dependent and deliberately not asserted", above).
    // The 90 s literal that stood here was the heaviest COMPILE budget in the
    // suite wearing a benchmark's clothes — `vilan/benchmarks` is the biggest
    // project any test builds (13.4 s on an idle 16-core box, 24.5 s at load
    // average ~28) and the benchmark workload itself runs in ~0.2 s of that. It
    // failed at exactly 90.0 s on both CI runners in v0.32.0. E40.
    let liveness = support::run_liveness();
    let deadline = Instant::now() + liveness;
    loop {
        match child.try_wait().expect("poll vilan run") {
            Some(_status) => break,
            None if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "the benchmarks build+run did not finish within {liveness:?} \
                     (a liveness bound, {:?} per reference compile on this machine)",
                    support::reference_compile()
                );
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(
        stderr.trim().is_empty(),
        "benchmarks wrote to stderr:\n{stderr}\nstdout:\n{stdout}"
    );

    // E33: the benchmark program binds its four RPC/duplex servers on port
    // 0 (throughput.vl's http-json, http-binary, and ws-multiplex servers;
    // realtime.vl's fan-out server) and announces each `[port] <n>` — the
    // same read-back precedent as `http_port.rs`'s `Server.port()` pin.
    // Reading them back here proves the migration actually happened (not a
    // silently-reintroduced fixed literal) and that every bind got a real,
    // distinct, OS-assigned port.
    let announced_ports: Vec<u16> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("[port] "))
        .map(|port_text| {
            port_text.trim().parse().unwrap_or_else(|error| {
                panic!("`[port]` line did not carry a number: {port_text:?} ({error})")
            })
        })
        .collect();
    assert_eq!(
        announced_ports.len(),
        4,
        "expected 4 `[port]` announcements (3 in throughput.vl, 1 in realtime.vl), got {announced_ports:?} in:\n{stdout}"
    );
    for port in &announced_ports {
        assert_ne!(
            *port, 0,
            "port 0 was reported back instead of the bound one"
        );
    }
    let mut distinct_ports = announced_ports.clone();
    distinct_ports.sort_unstable();
    distinct_ports.dedup();
    assert_eq!(
        distinct_ports.len(),
        announced_ports.len(),
        "the four servers should each get their own OS-assigned port, got {announced_ports:?}"
    );

    for expected in [
        "== payload sizes ==",
        "== coalescing (update frames counted at the wire) ==",
        "subscribe -> 1 update frame(s)",
        "100 lone sets -> 100 update frames",
        "100 sets in one batch -> 1 update frame(s)",
        "3 writes in one rpc handler -> 1 update frame(s)",
        "deliveries observed by the subscriber = 103",
        "== rpc round-trip throughput (sequential) ==",
        "== realtime fan-out (sse + post, 3 sessions, 50 mutations) ==",
        "per-session update frames: 51 51 51",
        "done",
    ] {
        assert!(
            stdout.contains(expected),
            "missing `{expected}` in benchmark output:\n{stdout}"
        );
    }
}
