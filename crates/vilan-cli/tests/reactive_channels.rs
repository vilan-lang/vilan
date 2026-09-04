//! The reactive protocol's channel table, from both ends (tracker A41).
//!
//! `[expose]` mints one channel per field per connection, positionally, and
//! everything about it is settled at `__attach` time — which is why the
//! DYNAMIC path (an rpc method minting a channel at runtime with the public
//! `session_of` + `ReactiveServer::expose`, the shape a per-row or per-thread
//! subscription needs) had no pins at all. These are its two: what an
//! `Unsubscribe` may and may not withdraw, and what a mirror minted from a
//! runtime channel id must say once its connection has been replaced.
//!
//! In-process on a `duplex_pair`, deliberately: both defects are in the tables,
//! not on the wire, and a socket would only add ways for the pin to be flaky.
//! The reconnect half — where the fresh session's ids are the point — is
//! pinned over real processes in `transport_robustness.rs`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod support;

fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vilan_reactive_channels_{tag}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// Build and run `source` as a whole program under a liveness bound, and
/// return its stdout (the same bound, and the same reason for it, as
/// `service_layer.rs`).
fn run_program(tag: &str, source: &str) -> String {
    let dir = temp_project(tag);
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(&dir, "src/main.vl", source);
    let liveness = support::run_liveness();
    let mut child = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn vilan run");
    let deadline = Instant::now() + liveness;
    loop {
        match child.try_wait().expect("poll vilan run") {
            Some(_status) => break,
            None if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the build+run did not exit within {liveness:?}");
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
        "the program wrote to stderr:\n{stderr}\n--- stdout ---\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
    stdout
}

/// One session and one client over an in-process duplex, driving a single
/// exposed cell through the whole capability lifecycle.
const CHANNEL_LIFECYCLE: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell };
import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };

fun main() {
	let (app_end, wire_end) = duplex_pair();
	let session = ReactiveServer::new(wire_end, json_codec());
	let client = ReactiveClient::new(app_end, json_codec());
	let cell: SignalCell<i32> = Signal::new(1);
	let channel = session.expose(cell);
	let mirror: RemoteSource<i32> = client.source(channel);

	// One lease, taken and released: `Unsubscribe` reaches the session.
	let first = mirror.sub(|value| print(i"first:{value}"));
	cell.set(2);
	first.dispose();

	// A second lease on the SAME channel — a remount. The capability must
	// still be there, or the mirror is silently dead from here on.
	let second = mirror.sub(|value| print(i"second:{value}"));
	cell.set(3);
	second.dispose();

	// Revoked: the capability is gone, so nothing this cell does afterwards
	// can reach a subscriber of that channel again.
	session.revoke(channel);
	let third = mirror.sub(|value| print(i"third:{value}"));
	cell.set(4);
	third.dispose();

	// The mirror was minted from a runtime channel id, so a replaced
	// connection invalidates it: back to `Waiting`, and no stale value.
	client.invalidate_dynamic();
	print(i"status:{mirror.status().get().debug()}");
	let fourth = mirror.sub(|value| print(i"stale:{value}"));
	fourth.dispose();
	print("done");
}
"#;

#[test]
fn an_unsubscribe_keeps_the_capability_and_revoke_withdraws_it() {
    // A41's first hole and the guard that shapes its fix. `ReactiveServer::stop`
    // disposed the live forward and left the `sources` entry, so a dynamic
    // exposure accumulated one retained starter — and its source — per channel
    // ever minted, until the connection died. The obvious fix, dropping the
    // entry in `stop`, is WRONG and this pin is why: an `Unsubscribe` is
    // client-local demand reaching zero (a view unmounting, a row scrolling
    // out), and `RemoteSource::acquire` re-`Subscribe`s on the same id when
    // demand returns. Dropping the capability there makes every remount
    // silently dead — `second:3` is the line that disappears.
    //
    // So the withdrawal is its own verb: `revoke` stops the forward AND drops
    // the capability, which is what the dynamic path calls when it is done
    // with an id.
    let stdout = run_program("lifecycle", CHANNEL_LIFECYCLE);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            // The first lease seeds from the current value and follows it.
            "first:1",
            "first:2",
            // The remount re-subscribes on the same channel: the server sends
            // the current value again, and updates flow as before.
            "second:2",
            "second:3",
            // After `revoke`: the cached value is still local (nothing
            // forgets it), but `cell.set(4)` reaches nobody.
            "third:3",
            // Invalidated: not `Ready` with a stale 3.
            "status:Waiting",
            "done",
        ],
        "the channel lifecycle went differently:\n{stdout}"
    );
    assert!(
        !stdout.contains("third:4"),
        "a revoked channel must not deliver again:\n{stdout}"
    );
    assert!(
        !stdout.contains("stale:"),
        "an invalidated mirror must hold no value to hand a fresh subscriber:\n{stdout}"
    );
}
