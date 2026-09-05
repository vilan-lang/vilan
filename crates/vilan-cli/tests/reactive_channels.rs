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

// --- The keyed half (A39) ---------------------------------------------------

/// THE EXHIBIT: a message platform, N channels x M messages, watched four ways
/// at once over four metered wires. Every shape sees the SAME store and the
/// same four events; what differs is only what each is told about them.
const MESSAGE_PLATFORM: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell, Source };
import std::rpc::{ DuplexEnd, KeyedSource, ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };
import std::shared::Shared;
import std::wire::{ Frame, Keyed, Wire };

[derive(Wire, PartialEq)]
struct Message {
	id: str,
	channel: i32,
	author: str,
	body: str,
}

impl Message with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

fun frame_bytes(frame: Frame): i32 {
	match frame {
		Frame::Text(let value) => value.len(),
		Frame::Binary(let bytes) => bytes.len(),
	}
}

/// A duplex pair with the server→client leg metered.
fun metered_link(down: Shared<i32>): (DuplexEnd, DuplexEnd) {
	let (client_end, client_relay) = duplex_pair();
	let (server_end, server_relay) = duplex_pair();
	client_relay.on_frame(|frame| server_relay.send(frame));
	server_relay.on_frame(|frame| {
		down.write() = down.read() + frame_bytes(frame);
		client_relay.send(frame);
	});
	(client_end, server_end)
}

fun corpus(channels: i32, per_channel: i32): List<Message> {
	mut all: List<Message> = [];
	mut channel = 0;
	for channel < channels {
		mut index = 0;
		for index < per_channel {
			all.push(Message {
				id = i"c{channel}-m{index}",
				channel,
				author = "reed",
				body = "the quick brown fox jumps over the lazy dog, repeatedly and at length",
			});
			index += 1;
		}
		channel += 1;
	}
	all
}

fun main() {
	let channels = 20;
	let per_channel = 50;
	let store: SignalCell<List<Message>> = Signal::new(corpus(channels, per_channel));
	let one_channel = store.map(|all: List<Message>| all.filter(|message| message.channel == 0));

	// (1) today's `[expose]` over the whole platform.
	let whole = Shared::new(0);
	let (whole_client_end, whole_server_end) = metered_link(whole);
	let whole_session = ReactiveServer::new(whole_server_end, json_codec());
	let whole_client = ReactiveClient::new(whole_client_end, json_codec());
	let whole_mirror: RemoteSource<List<Message>> = whole_client.source(whole_session.expose(store));
	let whole_lease = whole_mirror.sub(|_list| {});

	// (2) today's `[expose]` scoped to ONE channel — the O(M) baseline.
	let scoped = Shared::new(0);
	let (scoped_client_end, scoped_server_end) = metered_link(scoped);
	let scoped_session = ReactiveServer::new(scoped_server_end, json_codec());
	let scoped_client = ReactiveClient::new(scoped_client_end, json_codec());
	let scoped_mirror: RemoteSource<List<Message>> = scoped_client.source(scoped_session.expose(one_channel));
	let scoped_lease = scoped_mirror.sub(|_list| {});

	// (3) `[expose(keyed)]`, whole-collection lease.
	let keyed = Shared::new(0);
	let (keyed_client_end, keyed_server_end) = metered_link(keyed);
	let keyed_session = ReactiveServer::new(keyed_server_end, json_codec());
	let keyed_client = ReactiveClient::new(keyed_client_end, json_codec());
	let keyed_channel = keyed_session.expose_keyed(store, |message: Message| message.key());
	let keyed_mirror: KeyedSource<str, Message> = keyed_client.keyed_source(keyed_channel);
	let keyed_lease = keyed_mirror.sub(|_list| {});

	// (4) `[expose(keyed)]`, ONE key.
	let single = Shared::new(0);
	let (single_client_end, single_server_end) = metered_link(single);
	let single_session = ReactiveServer::new(single_server_end, json_codec());
	let single_client = ReactiveClient::new(single_client_end, json_codec());
	let single_channel = single_session.expose_keyed(store, |message: Message| message.key());
	let single_mirror: KeyedSource<str, Message> = single_client.keyed_source(single_channel);
	let single_lease = single_mirror.sub_key("c0-m0", |_value| {});

	report("seed", whole, scoped, keyed, single);

	// An EDIT of the message every shape is watching.
	reset(whole, scoped, keyed, single);
	store.update(|&mut list| {
		list[0] = Message { id = "c0-m0", channel = 0, author = "reed", body = "edited" };
	});
	report("edit", whole, scoped, keyed, single);

	// An EDIT of a message the single-key subscriber does not hold.
	reset(whole, scoped, keyed, single);
	store.update(|&mut list| {
		list[7] = Message { id = "c0-m7", channel = 0, author = "reed", body = "elsewhere" };
	});
	report("edit-elsewhere", whole, scoped, keyed, single);

	// A POST at the end of the busiest channel.
	reset(whole, scoped, keyed, single);
	store.update(|&mut list| {
		list.push(Message {
			id = "c19-new",
			channel = 19,
			author = "reed",
			body = "a brand new post arriving in the busiest channel on the platform",
		});
	});
	report("post", whole, scoped, keyed, single);

	// A DELETE inside channel 0.
	reset(whole, scoped, keyed, single);
	store.update(|&mut list| {
		let _gone = list.remove(3);
	});
	report("delete", whole, scoped, keyed, single);

	print(i"held whole={whole_mirror.get().unwrap_or([]).len()} keyed={keyed_mirror.get().unwrap_or([]).len()} single={single_mirror.get().unwrap_or([]).len()}");
	print(i"faults keyed={keyed_mirror.fault().is_some()} single={single_mirror.fault().is_some()}");
	whole_lease.dispose();
	scoped_lease.dispose();
	keyed_lease.dispose();
	single_lease.dispose();
	print("done");
}

fun reset(whole: Shared<i32>, scoped: Shared<i32>, keyed: Shared<i32>, single: Shared<i32>) {
	whole.write() = 0;
	scoped.write() = 0;
	keyed.write() = 0;
	single.write() = 0;
}

fun report(label: str, whole: Shared<i32>, scoped: Shared<i32>, keyed: Shared<i32>, single: Shared<i32>) {
	print(i"{label} whole={whole.read()} scoped={scoped.read()} keyed={keyed.read()} single={single.read()}");
}
"#;

#[test]
fn a_keyed_channel_costs_the_change_where_a_plain_one_costs_the_collection() {
    // A39's whole point, measured. `[expose]` forwards `source.sub(|v|
    // send(encode_update(.., v)))` — the WHOLE value, every change — so a
    // platform holding twenty channels of fifty messages resends all twenty
    // thousand-odd fields because one message was edited. A keyed channel
    // diffs the two snapshots by key and sends the ops.
    //
    // The four columns are the four exposure shapes over one store:
    //   whole  — `[expose]` over the platform         (the O(N*M) baseline)
    //   scoped — `[expose]` over ONE channel's list   (the O(M) baseline)
    //   keyed  — `[expose(keyed)]`, whole-collection lease
    //   single — `[expose(keyed)]`, one key leased
    //
    // The assertions are RATIOS and ceilings, not exact byte counts: the point
    // is the complexity class, and an exact count would break on any harmless
    // change to the fixture's prose.
    let stdout = run_program("platform", MESSAGE_PLATFORM);
    let rows = parse_rows(&stdout);
    let seed = rows["seed"];
    let edit = rows["edit"];
    let elsewhere = rows["edit-elsewhere"];
    let post = rows["post"];
    let delete = rows["delete"];

    // The seeds: a keyed channel is not cheaper to OPEN — it sends the same
    // collection once, as a `Reset`. Leasing one key is what makes the seed
    // small, and that is the honest sentence.
    assert!(
        seed.keyed > 100_000 && seed.single < 1_000,
        "the seeds went differently:\n{stdout}"
    );

    // One EDIT: ~1 KB instead of ~120 KB, three orders of magnitude, and the
    // single-key subscriber pays the same because it is watching that key.
    assert!(
        edit.whole > 100_000 && edit.keyed < 1_000 && edit.single < 1_000,
        "one edit did not collapse:\n{stdout}"
    );
    assert!(
        edit.whole / edit.keyed.max(1) > 500,
        "one edit's saving was not the class this item exists for:\n{stdout}"
    );

    // An edit ELSEWHERE: the per-key subscriber is told NOTHING. That is the
    // per-key half of the protocol, and zero is the number that proves it.
    assert_eq!(
        elsewhere.single, 0,
        "a per-key subscription heard about another key's edit:\n{stdout}"
    );

    // One POST: O(1) against the O(M) single-channel baseline too, not just
    // against the whole platform.
    assert!(
        post.keyed < 400 && post.scoped > 5_000 && post.whole > 100_000,
        "one post did not collapse:\n{stdout}"
    );
    assert_eq!(
        post.single, 0,
        "a post in another channel reached a per-key subscription:\n{stdout}"
    );

    // One DELETE: a `Remove` is just the key.
    assert!(
        delete.keyed < 100 && delete.whole > 100_000,
        "one delete did not collapse:\n{stdout}"
    );

    // And the mirrors are CORRECT, which is the other half of the claim: the
    // keyed mirror holds every message, the per-key one holds exactly its own.
    assert!(
        stdout.contains("held whole=1000 keyed=1000 single=1"),
        "the mirrors did not agree with the store:\n{stdout}"
    );
    assert!(
        stdout.contains("faults keyed=false single=false"),
        "a patch was refused by the mirror it was built for:\n{stdout}"
    );
}

/// One metered row of the exhibit's table.
#[derive(Clone, Copy)]
struct Row {
    whole: i64,
    scoped: i64,
    keyed: i64,
    single: i64,
}

/// `label whole=.. scoped=.. keyed=.. single=..` per line.
fn parse_rows(stdout: &str) -> std::collections::HashMap<String, Row> {
    let mut rows = std::collections::HashMap::new();
    for line in stdout.lines() {
        let line = line.trim();
        let mut fields = line.split_whitespace();
        let Some(label) = fields.next() else { continue };
        let mut values = Vec::new();
        for field in fields {
            let Some((_name, value)) = field.split_once('=') else {
                values.clear();
                break;
            };
            let Ok(value) = value.parse::<i64>() else {
                values.clear();
                break;
            };
            values.push(value);
        }
        if let [whole, scoped, keyed, single] = values[..] {
            rows.insert(
                label.to_string(),
                Row {
                    whole,
                    scoped,
                    keyed,
                    single,
                },
            );
        }
    }
    assert!(
        rows.contains_key("edit"),
        "the exhibit printed no table:\n{stdout}"
    );
    rows
}

/// Per-key demand through its whole lifecycle: seed, follow, release, remount,
/// and the key leaving the collection under the subscriber's feet.
const PER_KEY_DEMAND: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell };
import std::rpc::{ DuplexEnd, KeyedSource, ReactiveClient, ReactiveServer, duplex_pair };
import std::shared::Shared;
import std::wire::{ Frame, Keyed, Wire };

[derive(Wire, PartialEq)]
struct Row {
	id: str,
	value: i32,
}

impl Row with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

fun frame_bytes(frame: Frame): i32 {
	match frame {
		Frame::Text(let value) => value.len(),
		Frame::Binary(let bytes) => bytes.len(),
	}
}

fun metered_link(down: Shared<i32>): (DuplexEnd, DuplexEnd) {
	let (client_end, client_relay) = duplex_pair();
	let (server_end, server_relay) = duplex_pair();
	client_relay.on_frame(|frame| server_relay.send(frame));
	server_relay.on_frame(|frame| {
		down.write() = down.read() + frame_bytes(frame);
		client_relay.send(frame);
	});
	(client_end, server_end)
}

fun rows(a: i32, b: i32, c: i32): List<Row> {
	[Row { id = "a", value = a }, Row { id = "b", value = b }, Row { id = "c", value = c }]
}

fun main() {
	let down: Shared<i32> = Shared::new(0);
	let (client_end, server_end) = metered_link(down);
	let session = ReactiveServer::new(server_end, json_codec());
	let client = ReactiveClient::new(client_end, json_codec());
	let store: SignalCell<List<Row>> = Signal::new(rows(1, 1, 1));
	let channel = session.expose_keyed(store, |row: Row| row.key());
	let mirror: KeyedSource<str, Row> = client.keyed_source(channel);

	let watch_b = mirror.sub_key("b", |value| match value {
		Some(let row) => print(i"b:{row.value}"),
		None => print("b:absent"),
	});
	print(i"held-after-seed:{mirror.get().unwrap_or([]).len()}");
	store.set(rows(2, 2, 2));
	print(i"status:{mirror.status().get().debug()}");

	// The lease reaches zero: the key is released server-side.
	watch_b.dispose();
	down.write() = 0;
	store.set(rows(3, 3, 3));
	print(i"after-release-bytes:{down.read()}");

	// Demand returns on the same key: the server re-seeds it with an
	// `Insert`, which must REPLACE rather than duplicate.
	let again = mirror.sub_key("b", |value| match value {
		Some(let row) => print(i"b-again:{row.value}"),
		None => print("b-again:absent"),
	});
	print(i"held-after-remount:{mirror.get().unwrap_or([]).len()}");
	store.set(rows(4, 4, 4));

	// A key that leaves the collection reads absent.
	store.set([Row { id = "a", value = 5 }]);
	print(i"held-after-remove:{mirror.get().unwrap_or([]).len()}");
	again.dispose();
	print(i"fault:{mirror.fault().is_some()}");
	print("done");
}
"#;

#[test]
fn a_per_key_lease_releases_its_key_at_zero_and_reseeds_when_demand_returns() {
    // The counted lease, per KEY rather than per channel. `Unsubscribe(channel,
    // Some(key))` releases that key's forward alone, and the proof is bytes:
    // after the release, a change to the very row that was being watched puts
    // NOTHING on the wire. Then demand returns on the same key and the server
    // re-seeds it with an `Insert` — which must REPLACE rather than duplicate,
    // or a remounted row would appear twice in the mirror.
    let stdout = run_program("perkey", PER_KEY_DEMAND);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            // The seed carries only the leased key, so the mirror holds one
            // element out of three.
            "b:1",
            "held-after-seed:1",
            "b:2",
            "status:Ready",
            // The lease reached zero: the server released the key.
            "after-release-bytes:0",
            // Demand returned: the re-seed is the value as of NOW, not the
            // value the mirror last saw.
            "b-again:3",
            "held-after-remount:1",
            "b-again:4",
            // The key left the collection: `Remove`, and the observer reads
            // absent rather than stale.
            "b-again:absent",
            "held-after-remove:0",
            "fault:false",
            "done",
        ],
        "the per-key lifecycle went differently:\n{stdout}"
    );
}

/// A server that patches a key the mirror does not hold.
const STRAY_PATCH: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell };
import std::rpc::{ KeyedSource, ReactiveClient, ReactiveServer, duplex_pair, encode_patch };
import std::wire::{ Delta, Keyed, Serializer, Wire };

[derive(Wire, PartialEq)]
struct Row {
	id: str,
	value: i32,
}

impl Row with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

fun render(list: List<Row>): str {
	mut out = "";
	for row in list {
		out = out + row.id + "=" + i"{row.value}" + " ";
	}
	out
}

fun main() {
	let (app_end, wire_end) = duplex_pair();
	let session = ReactiveServer::new(wire_end, json_codec());
	let client = ReactiveClient::new(app_end, json_codec());
	let store: SignalCell<List<Row>> = Signal::new([Row { id = "a", value = 1 }]);
	let channel = session.expose_keyed(store, |row: Row| row.key());
	let mirror: KeyedSource<str, Row> = client.keyed_source(channel);
	let watch = mirror.sub(|list| print(i"seen:{render(list)}"));
	print(i"fault-before:{mirror.fault().unwrap_or("none")}");

	// A server that patches a key this mirror does not hold is a PROTOCOL
	// error: the op is not applied, and the mirror says so once.
	let codec = json_codec();
	let stray: List<Delta<str, Row>> = [Delta::Update("zzz", Row { id = "zzz", value = 9 })];
	wire_end.send(encode_patch(codec, channel, |serializer: Serializer| {
		serializer.begin_list(stray.len());
		for op in stray {
			op.describe(serializer);
		}
		serializer.end_list();
	}));
	print(i"fault-after-update:{mirror.fault().unwrap_or("none")}");
	print(i"held:{render(mirror.get().unwrap_or([]))}");

	// Sticky: the first fault is the one kept.
	let second: List<Delta<str, Row>> = [Delta::Remove("yyy")];
	wire_end.send(encode_patch(codec, channel, |serializer: Serializer| {
		serializer.begin_list(second.len());
		for op in second {
			op.describe(serializer);
		}
		serializer.end_list();
	}));
	print(i"fault-after-remove:{mirror.fault().unwrap_or("none")}");

	// A well-formed change still lands: the mirror is not wedged.
	store.set([Row { id = "a", value = 2 }, Row { id = "b", value = 3 }]);
	print(i"held-after:{render(mirror.get().unwrap_or([]))}");
	watch.dispose();
	print("done");
}
"#;

#[test]
fn a_patch_naming_a_key_the_mirror_does_not_hold_is_a_reported_protocol_error() {
    // `Update` and `Remove` name a key that must already be there. An absent
    // one is a SERVER bug or a lost frame — never something application code
    // can cause — so the mirror refuses the op rather than inventing an
    // element or silently dropping it, and says so once.
    //
    // Three properties, all pinned here: the fault is reported, the offending
    // op is NOT applied (the mirror keeps the state it could account for), and
    // the fault is STICKY — the second stray op does not overwrite the first,
    // because everything after the first is a consequence.
    let stdout = run_program("stray", STRAY_PATCH);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "seen:a=1",
            "fault-before:none",
            "seen:a=1",
            "fault-after-update:an Update named a key the mirror does not hold",
            "held:a=1",
            "seen:a=1",
            // Sticky: the `Remove`'s own fault does not replace the `Update`'s.
            "fault-after-remove:an Update named a key the mirror does not hold",
            "seen:a=2 b=3",
            // And the mirror is not wedged: real changes still land.
            "held-after:a=2 b=3",
            "done",
        ],
        "the stray patch was handled differently:\n{stdout}"
    );
}

/// A keyed mirror across a replaced connection.
const KEYED_REBIND: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell };
import std::rpc::{ KeyedSource, ReactiveClient, ReactiveServer, duplex_pair };
import std::wire::{ Keyed, Wire };

[derive(Wire, PartialEq)]
struct Row {
	id: str,
	value: i32,
}

impl Row with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

fun render(list: List<Row>): str {
	mut out = "";
	for row in list {
		out = out + row.id + "=" + i"{row.value}" + " ";
	}
	out
}

fun main() {
	let (app_end, wire_end) = duplex_pair();
	let session = ReactiveServer::new(wire_end, json_codec());
	let client = ReactiveClient::new(app_end, json_codec());
	let store: SignalCell<List<Row>> = Signal::new([
		Row { id = "a", value = 1 },
		Row { id = "b", value = 1 },
	]);
	let channel = session.expose_keyed(store, |row: Row| row.key());
	let mirror: KeyedSource<str, Row> = client.keyed_source(channel);
	let watch_b = mirror.sub_key("b", |value| match value {
		Some(let row) => print(i"b:{row.value}"),
		None => print("b:absent"),
	});

	// The connection DROPS: the session goes, and nothing this store does
	// reaches the mirror any more.
	session.dispose();
	store.set([Row { id = "a", value = 7 }]);
	print(i"while-down:{render(mirror.get().unwrap_or([]))}");

	// It comes back: a fresh session on the same wire mints a fresh channel
	// for the same source, and the mirror is rebound onto it — the shape
	// `reattach_mirrors` drives for a generated client.
	let fresh_session = ReactiveServer::new(wire_end, json_codec());
	let fresh_channel = fresh_session.expose_keyed(store, |row: Row| row.key());
	print(i"fresh-differs:{fresh_channel != channel}");
	mirror.rebind(fresh_channel);
	// The key it still holds a lease on is re-subscribed, and `b` is gone,
	// so it re-seeds as ABSENT rather than as the stale row it was holding.
	print(i"after-rebind:{render(mirror.get().unwrap_or([]))}");
	store.set([Row { id = "a", value = 8 }, Row { id = "b", value = 9 }]);
	print(i"after-return:{render(mirror.get().unwrap_or([]))}");
	watch_b.dispose();
	print("done");
}
"#;

#[test]
fn a_keyed_mirror_resubscribes_the_keys_it_holds_after_a_rebind() {
    // A41 left the dynamic path with `invalidate_dynamic`: a mirror minted from
    // a runtime channel id could not be rebound, because no protocol form
    // existed to ask for it again — "that is A39". This is the form. A keyed
    // mirror rebinds like any other, and re-subscribes every demand it still
    // holds: the whole-collection lease if it has one, and each counted key.
    //
    // The sharp edge is the CACHE. A plain mirror may keep its last value
    // across a rebind, because the fresh subscription resends the whole value.
    // A keyed forward reseeds only what it is asked for, so an element deleted
    // while the connection was down would survive as a ghost no later op would
    // ever name. `rebind` clears first, and this pin is that clearing: while
    // the connection is down the mirror still holds the stale `b=1`, and after
    // the rebind it holds NOTHING, because `b` is gone.
    let stdout = run_program("keyedrebind", KEYED_REBIND);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "b:1",
            // Nothing reached the mirror while the session was disposed.
            "while-down:b=1",
            "fresh-differs:true",
            // The rebind empties the mirror, then the fresh forward re-seeds
            // the key it still holds — as ABSENT, because `b` was deleted.
            "b:absent",
            "b:absent",
            "after-rebind:",
            "b:9",
            "after-return:b=9",
            "done",
        ],
        "the keyed rebind went differently:\n{stdout}"
    );
    assert!(
        !stdout.contains("after-rebind:b=1"),
        "a keyed mirror kept a ghost across a reconnect:\n{stdout}"
    );
}
