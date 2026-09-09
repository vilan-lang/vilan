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

// --- A51: what `keyed_diff` costs in CPU, per change per connection ---------
//
// A39 measured the keyed channel's BYTES and found them O(change): an edit in a
// 1,000-message platform went from 123,753 bytes to 95. It also recorded, and
// did not measure, the other half — `keyed_diff` re-keys two whole snapshots on
// every change, so the CPU is O(N) per change, and it is paid once per
// SUBSCRIBED CONNECTION because each connection's forward runs its own diff.
// The client half is O(N) too: `KeyedSource::apply` reaches `index_of` per op,
// which is a linear scan of the mirror.
//
// This is the measurement A51 asks for BEFORE anything incremental is built.
// Two scales (1,000 and 10,000 rows) crossed with two connection counts (1 and
// 8), each run twice — once making the changes, once making none — so the
// seed, the process start and the node runtime subtract out and what is left is
// the changes alone.
//
// CPU, not wall: `getrusage(RUSAGE_CHILDREN)` around each `node` run, which is
// the child's user+system time and accrues nothing to preemption. Every row
// stamps the 1-minute loadavg anyway, so a row measured on a busy box says so.
// `#[ignore]`d — it spawns eight node processes and is a measurement, not a
// gate. Run it with:
//
// ```text
// cargo nextest run -p vilan-cli --test reactive_channels --run-ignored \
//     ignored-only -E 'test(keyed_diff)' --no-capture
// ```

/// The measured program: `rows` rows behind one keyed channel, `connections`
/// independent sessions each holding a whole-collection lease, and `changes`
/// single-element edits. Every parameter is substituted, so the two runs of a
/// pair differ in `changes` and in nothing else.
const KEYED_DIFF_COST: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell, Subscription };
import std::rpc::{ KeyedSource, ReactiveClient, ReactiveServer, duplex_pair };
import std::wire::{ Keyed, Wire };

[derive(Wire, PartialEq, Debug)]
struct Row {
	id: str,
	value: i32,
	body: str,
}

impl Row with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

fun corpus(count: i32): List<Row> {
	mut all: List<Row> = [];
	mut index = 0;
	for index < count {
		all.push(Row {
			id = i"row-{index}",
			value = index,
			body = "the quick brown fox jumps over the lazy dog, repeatedly and at length",
		});
		index += 1;
	}
	all
}

fun main() {
	let rows = __ROWS__;
	let connections = __CONNECTIONS__;
	let changes = __CHANGES__;
	let store: SignalCell<List<Row>> = Signal::new(corpus(rows));

	mut leases: List<Subscription> = [];
	mut opened = 0;
	for opened < connections {
		let (client_end, server_end) = duplex_pair();
		let session = ReactiveServer::new(server_end, json_codec());
		let client = ReactiveClient::new(client_end, json_codec());
		let channel = session.expose_keyed(store, |row: Row| row.key());
		let mirror: KeyedSource<str, Row> = client.keyed_source(channel);
		leases.push(mirror.sub(|_list| {}));
		opened += 1;
	}

	mut made = 0;
	for made < changes {
		// One element edited in place: the smallest change there is, and the
		// one A39's byte measurement priced at 95 bytes.
		store.update(|&mut list| {
			list[0] = Row {
				id = "row-0",
				value = made,
				body = "the quick brown fox jumps over the lazy dog, repeatedly and at length",
			};
		});
		made += 1;
	}

	for lease in leases {
		lease.dispose();
	}
	print(i"rows={rows} connections={connections} changes={changes}");
}
"#;

/// The child processes' accumulated CPU (user + system) so far — the clock this
/// measurement reads. `RUSAGE_CHILDREN` counts REAPED children, and a nextest
/// test owns its process, so a delta taken around one `Command::output` is that
/// one child and nothing else.
#[cfg(unix)]
fn children_cpu_now() -> Option<Duration> {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: `getrusage` writes the `rusage` we hand it and reads nothing
    // else; the pointer is to a live local.
    let result = unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, &mut usage) };
    if result != 0 {
        return None;
    }
    let of = |time: libc::timeval| {
        Duration::from_secs(time.tv_sec.max(0) as u64)
            + Duration::from_micros(time.tv_usec.max(0) as u64)
    };
    Some(of(usage.ru_utime) + of(usage.ru_stime))
}

/// Every other host: no children-CPU clock, so the measurement DECLINES rather
/// than reports a wall-clock number under a CPU heading.
#[cfg(not(unix))]
fn children_cpu_now() -> Option<Duration> {
    None
}

fn loadavg_1m() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|text| text.split_whitespace().next().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

#[test]
#[ignore = "A51: a measurement, not a gate — it spawns eight node processes and reports numbers"]
fn keyed_diff_cpu_per_change_per_connection() {
    let Some(_probe) = children_cpu_now() else {
        eprintln!("PERF declined: this host exposes no children-CPU clock");
        return;
    };
    let dir = temp_project("keyed_diff_cost");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );

    // One `vilan build` per shape, then `node` on the bundle — so the compile
    // is never inside a measured span.
    let measure = |rows: i32, connections: i32, changes: i32| -> Duration {
        write(
            &dir,
            "src/main.vl",
            &KEYED_DIFF_COST
                .replace("__ROWS__", &rows.to_string())
                .replace("__CONNECTIONS__", &connections.to_string())
                .replace("__CHANGES__", &changes.to_string()),
        );
        let built = Command::new(env!("CARGO_BIN_EXE_vilan"))
            .args(["build", dir.to_str().unwrap()])
            .output()
            .expect("build the measured program");
        assert!(
            built.status.success(),
            "the measured program did not build:\n{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let bundle = dir.join("src").join("main.mjs");
        assert!(bundle.exists(), "the bundle is not at {}", bundle.display());
        let before = children_cpu_now().expect("a children-CPU clock");
        let ran = Command::new("node")
            .arg(&bundle)
            .output()
            .expect("run the measured program");
        let after = children_cpu_now().expect("a children-CPU clock");
        assert!(
            ran.status.success(),
            "the measured program did not run:\n{}",
            String::from_utf8_lossy(&ran.stderr)
        );
        after.saturating_sub(before)
    };

    const CHANGES: i32 = 200;
    println!("PERF loadavg-before {}", loadavg_1m());
    for rows in [1_000, 10_000] {
        for connections in [1, 8] {
            let idle = measure(rows, connections, 0);
            let busy = measure(rows, connections, CHANGES);
            let attributable = busy.saturating_sub(idle);
            let per =
                attributable.as_secs_f64() * 1000.0 / f64::from(CHANGES) / f64::from(connections);
            println!(
                "PERF {{\"section\":\"a51-keyed-diff\",\"rows\":{rows},\"connections\":{connections},\
                 \"changes\":{CHANGES},\"idle_ms\":{:.1},\"busy_ms\":{:.1},\
                 \"attributable_ms\":{:.1},\"ms_per_change_per_connection\":{per:.4},\
                 \"clock\":\"children-cpu\",\"load\":\"{}\"}}",
                idle.as_secs_f64() * 1000.0,
                busy.as_secs_f64() * 1000.0,
                attributable.as_secs_f64() * 1000.0,
                loadavg_1m()
            );
        }
    }
    println!("PERF loadavg-after {}", loadavg_1m());
    let _ = std::fs::remove_dir_all(&dir);
}

// --- A51: the `List<T>` keyed exposure gets its macro form ------------------

/// `[expose(keyed = str)] tasks: SignalCell<List<Task>>` — A39 refused this
/// shape and steered to `Map<K, V>` or the hand-wired `expose_keyed`, because a
/// keyed mirror is a `KeyedSource<K, T>` and a `List<T>` names only the
/// element. A51's ruling gives the key to the ATTRIBUTE, which is the one place
/// the author can write it and the expansion can read it before any type
/// resolves.
///
/// The claim is that the generated wiring IS the hand-written one, and it is
/// measured on the wire rather than argued: one source, two channels — one
/// reached through the generated `__attach`, one through a hand-written
/// `session.expose_keyed(tasks, |task| task.key())` — and every frame the two
/// put on their relays, for a seed and three changes, is compared after the
/// channel id (which is a fresh counter, not a shape) is normalized away.
const GENERATED_AGAINST_HAND_WIRED: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell };
import std::result::Result::{ self, Ok, Err };
import std::rpc::{
	DuplexEnd,
	KeyedSource,
	ReactiveClient,
	ReactiveServer,
	RpcError,
	call,
	duplex_pair,
	local_rpc,
	register_session,
};
import std::shared::Shared;
import std::wire::{ Frame, Keyed, Serializer, Wire };

[derive(Wire, PartialEq, Debug)]
struct Task { id: str, label: str }

impl Task with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

[service(StoreClient)]
struct Store {
	[expose(keyed = str)] tasks: SignalCell<List<Task>>,
}

impl Store {
	[rpc]
	fun touch(self): i32 {
		self.tasks.get().len()
	}
}

let tasks: SignalCell<List<Task>> = Signal::new([
	Task { id = "a", label = "alpha" },
	Task { id = "b", label = "beta" },
]);
let store: Store = Store { tasks };

fun text_of(frame: Frame): str {
	match frame {
		Frame::Text(let text) => text,
		Frame::Binary(let _bytes) => "<binary>",
	}
}

fun logged_pair(label: str): (DuplexEnd, DuplexEnd) {
	let (client_end, client_relay) = duplex_pair();
	let (server_end, server_relay) = duplex_pair();
	client_relay.on_frame(|frame| server_relay.send(frame));
	server_relay.on_frame(|frame| {
		print(i"{label} {text_of(frame)}");
		client_relay.send(frame);
	});
	(client_end, server_end)
}

fun main() {
	// (1) the GENERATED wiring, reached exactly as a client reaches it.
	let (gen_client_end, gen_server_end) = logged_pair("gen");
	register_session(1, gen_server_end, json_codec());
	let transport = local_rpc(store.dispatcher().into_protocol(json_codec()));
	let attached: Result<List<i32>, RpcError> = call(transport, json_codec(), "__attach", [|serializer: Serializer| 1.describe(serializer)]);
	let channels = attached.unwrap_or([]);
	print(i"gen-channel:{channels[0]}");
	let gen_client = ReactiveClient::new(gen_client_end, json_codec());
	let gen_mirror: KeyedSource<str, Task> = gen_client.keyed_source(channels[0]);
	let gen_lease = gen_mirror.sub(|_list| {});

	// (2) the HAND-WIRED call the macro writes, on the same source.
	let (hand_client_end, hand_server_end) = logged_pair("hand");
	let hand_session = ReactiveServer::new(hand_server_end, json_codec());
	let hand_channel = hand_session.expose_keyed(tasks, |task: Task| task.key());
	print(i"hand-channel:{hand_channel}");
	let hand_client = ReactiveClient::new(hand_client_end, json_codec());
	let hand_mirror: KeyedSource<str, Task> = hand_client.keyed_source(hand_channel);
	let hand_lease = hand_mirror.sub(|_list| {});

	tasks.update(|&mut list| { list[0] = Task { id = "a", label = "edited" }; });
	tasks.update(|&mut list| { list.push(Task { id = "c", label = "gamma" }); });
	tasks.update(|&mut list| { let _gone = list.remove(1); });

	print(i"held gen={gen_mirror.get().unwrap_or([]).len()} hand={hand_mirror.get().unwrap_or([]).len()}");
	gen_lease.dispose();
	hand_lease.dispose();
	print("done");
}
"#;

#[test]
fn the_generated_keyed_list_exposure_is_the_hand_wired_one_frame_for_frame() {
    let stdout = run_program("keyedlist", GENERATED_AGAINST_HAND_WIRED);
    let channel_of = |label: &str| -> String {
        stdout
            .lines()
            .map(str::trim)
            .find_map(|line| line.strip_prefix(label).map(str::to_string))
            .unwrap_or_else(|| panic!("`{label}` is missing from:\n{stdout}"))
    };
    let generated_channel = channel_of("gen-channel:");
    let hand_channel = channel_of("hand-channel:");
    assert_ne!(
        generated_channel, hand_channel,
        "the two channels must be distinct, or the comparison is vacuous"
    );
    // The channel id is a process-wide counter, so it is the one byte that
    // legitimately differs; everything else must match exactly.
    let frames = |label: &str, channel: &str| -> Vec<String> {
        stdout
            .lines()
            .map(str::trim)
            .filter_map(|line| line.strip_prefix(label))
            .map(|frame| frame.replace(&format!("[{channel},"), "[C,"))
            .collect()
    };
    let generated = frames("gen ", &generated_channel);
    let hand = frames("hand ", &hand_channel);
    assert!(
        !generated.is_empty(),
        "the generated channel put nothing on the wire:\n{stdout}"
    );
    assert_eq!(
        generated, hand,
        "the generated keyed exposure and the hand-wired one must be the same \
         channel, frame for frame:\n{stdout}"
    );
    // And the frames are the ones A39 pinned for the keyed shape: a `Reset`
    // seed, then one op per change, naming the key and nothing else.
    assert_eq!(
        generated,
        vec![
            "{\"Patch\":[C,[{\"Reset\":[{\"id\":\"a\",\"label\":\"alpha\"},{\"id\":\"b\",\"label\":\"beta\"}]}]]}",
            "{\"Patch\":[C,[{\"Update\":[\"a\",{\"id\":\"a\",\"label\":\"edited\"}]}]]}",
            "{\"Patch\":[C,[{\"Insert\":[\"c\",{\"id\":\"c\",\"label\":\"gamma\"},2]}]]}",
            "{\"Patch\":[C,[{\"Remove\":\"b\"}]]}",
        ],
        "the keyed frames moved:\n{stdout}"
    );
    assert!(
        stdout.contains("held gen=2 hand=2"),
        "both mirrors must hold the same collection:\n{stdout}"
    );
}

// --- A55: a keyed mirror IS a `Source<Option<List<T>>>` ---------------------

/// A52 gave `RemoteSource` its `Source` impl and left the keyed twin bare, so a
/// `KeyedSource<K, T>` could be observed only through its own inherent members:
/// A49's `on_change` was not on it, `effect` was not on it, and no generic
/// `S: Source<…>` function could take one — which is what a keyed handle in a
/// return position needs. A55 closes that seam on the SHIPPED counted lease
/// (`acquire`/`release`), and no second mechanism: the proof that it is the
/// same lease is BYTES on the wire, exactly as the per-key pin above measures
/// them.
///
/// Four claims: (1) two generic functions bounded on
/// `Source<Option<List<Row>>>` take the mirror at all — the call that did not
/// compile before A55; (2) the trait's own primitive `on_change` opens the
/// channel and its dispose closes it, measured by the bytes a later change
/// costs; (3) `effect_on_change`, a trait DEFAULT nothing on `KeyedSource`
/// declares, works through it; (4) R5's seam — `or([])` is still what a list
/// binding says, because the trait argument is the `Option` and a mirror told
/// nothing is not an empty collection.
const KEYED_MIRROR_IS_A_SOURCE: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell, Source, Subscription, batch, owner_scope, Owner };
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

/// Generic over the READ trait — the call that did not compile before A55.
fun held_by<S: Source<Option<List<Row>>>>(source: S): i32 {
	match source.get() {
		Some(let list) => list.len(),
		None => -1,
	}
}

/// The lazy attach, generically: `on_change` is the trait's primitive, so a
/// generic consumer reaches the mirror's counted one.
fun watch<S: Source<Option<List<Row>>>>(source: S, seen: SignalCell<i32>): Subscription {
	source.on_change(|value| match value {
		Some(let list) => seen.set(list.len()),
		None => seen.set(-1),
	})
}

fun main() {
	let down: Shared<i32> = Shared::new(0);
	let (client_end, server_end) = metered_link(down);
	let session = ReactiveServer::new(server_end, json_codec());
	let client = ReactiveClient::new(client_end, json_codec());
	let store: SignalCell<List<Row>> = Signal::new([Row { id = "a", value = 1 }]);
	let channel = session.expose_keyed(store, |row: Row| row.key());
	let mirror: KeyedSource<str, Row> = client.keyed_source(channel);

	// Passive: the trait's `get` opens nothing, exactly like the inherent one.
	print(i"unopened:{held_by(mirror)}");

	let seen: SignalCell<i32> = Signal::new(-2);
	let held = watch(mirror, seen);
	// The lease is the shipped one, so the seeding `Patch` came back INSIDE
	// the acquire — which is why `on_change` attaches before it takes it.
	print(i"watched:{held_by(mirror)}:{seen.get()}");

	store.update(|&mut list| { list.push(Row { id = "b", value = 2 }); });
	print(i"changed:{held_by(mirror)}:{seen.get()}");

	// Releasing the trait's subscription releases the same count: after the
	// settle the forward is gone, and a change costs NOTHING on the wire.
	held.dispose();
	batch(|| {});
	down.write() = 0;
	store.update(|&mut list| { list.push(Row { id = "c", value = 3 }); });
	print(i"after-release-bytes:{down.read()}");

	// `effect_on_change` is a trait DEFAULT nothing on `KeyedSource` declares:
	// it reaches the mirror only because the impl exists, and it dies with the
	// owner rather than with the tab.
	let scope = Owner::new();
	owner_scope.run(scope, || {
		mirror.effect_on_change(|value| match value {
			Some(let list) => print(i"effect:{list.len()}"),
			None => print("effect:none"),
		});
		// R5: the seam into a list binding is still `or([])` — the trait
		// argument is the Option, which is the truth about a mirror that has
		// been told nothing.
		let rendered = mirror.or([]);
		print(i"or-len:{rendered.get().len()}");
	});
	store.update(|&mut list| { list.push(Row { id = "d", value = 4 }); });
	scope.dispose();
	batch(|| {});
	down.write() = 0;
	store.update(|&mut list| { list.push(Row { id = "e", value = 5 }); });
	print(i"after-owner-dispose-bytes:{down.read()}");
	print("done");
}
"#;

#[test]
fn a_keyed_mirror_is_a_source_over_the_shipped_counted_lease() {
    let stdout = run_program("keyedsource", KEYED_MIRROR_IS_A_SOURCE);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert!(
        lines.contains(&"done"),
        "the program did not finish:\n{stdout}"
    );
    // Passive before anything asks: no value, and no forward to pay for.
    assert!(
        lines.contains(&"unopened:-1"),
        "the trait's `get` must open nothing:\n{stdout}"
    );
    // The trait's own primitive took the lease, and the seed came back inside
    // it — so the generic observer already saw the collection.
    assert!(
        lines.contains(&"watched:1:1"),
        "`on_change` through the trait must take the shipped lease and seed:\n{stdout}"
    );
    assert!(
        lines.contains(&"changed:2:2"),
        "a change must reach a generic `Source` observer:\n{stdout}"
    );
    // The same lease, released: nothing on the wire for the next change.
    assert!(
        lines.contains(&"after-release-bytes:0"),
        "disposing the trait's subscription must release the SHIPPED count \
         (a later change would otherwise still be forwarded):\n{stdout}"
    );
    // A trait default nothing on `KeyedSource` declares, reaching the mirror.
    assert!(
        lines.contains(&"effect:4") || lines.contains(&"effect:3"),
        "`effect_on_change` (a `Source` default) must reach a keyed mirror:\n{stdout}"
    );
    // R5: `or([])` is what an unseeded list binding says, and it is a lease.
    assert!(
        lines.iter().any(|line| line.starts_with("or-len:")),
        "`or([])` must still be the list-binding seam:\n{stdout}"
    );
    assert!(
        lines.contains(&"after-owner-dispose-bytes:0"),
        "the owner's dispose must release every lease the extent took:\n{stdout}"
    );
}

// --- A54: the delta-producing cell -----------------------------------------

/// `keyed_forward` over a `SignalCell<List<T>>` can only learn what changed by
/// comparing two snapshots, so a keyed channel costs O(N) per change per
/// SUBSCRIBED CONNECTION however small the change is (A51's measurement: 1.08
/// ms per change at 1,000 rows, 8.83 at 10,000). A54's answer is a cell whose
/// WRITES are the deltas.
///
/// The whole claim of the cell is that it changes the COST and nothing else, so
/// the pin is an identity: one program, two channels over the same edits — one
/// fed by a `SignalCell<List<Task>>` through `expose_keyed`, one by a
/// `KeyedCell<str, Task>` through `expose_keyed_cell` — and every frame the two
/// put on their relays compared after the channel id (a fresh counter, not a
/// shape) is normalized away. Both demands are covered: the whole collection,
/// and one key. The `SignalCell` half is also the CONTROL — it must keep
/// patching exactly as A39 pinned it.
const KEYED_CELL_AGAINST_THE_DIFF: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell, batch };
import std::rpc::{ DuplexEnd, KeyedCell, KeyedSource, ReactiveClient, ReactiveServer, duplex_pair };
import std::wire::{ Frame, Keyed, Wire };

[derive(Wire, PartialEq, Debug)]
struct Task { id: str, label: str }

impl Task with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

fun text_of(frame: Frame): str {
	match frame {
		Frame::Text(let text) => text,
		Frame::Binary(let _bytes) => "<binary>",
	}
}

fun logged_pair(label: str): (DuplexEnd, DuplexEnd) {
	let (client_end, client_relay) = duplex_pair();
	let (server_end, server_relay) = duplex_pair();
	client_relay.on_frame(|frame| server_relay.send(frame));
	server_relay.on_frame(|frame| {
		print(i"{label} {text_of(frame)}");
		client_relay.send(frame);
	});
	(client_end, server_end)
}

fun seed(): List<Task> {
	[Task { id = "a", label = "alpha" }, Task { id = "b", label = "beta" }]
}

fun main() {
	// (1) the DIFF path — the shipped `SignalCell<List<T>>` exposure.
	let store: SignalCell<List<Task>> = Signal::new(seed());
	let (diff_client_end, diff_server_end) = logged_pair("diff");
	let diff_session = ReactiveServer::new(diff_server_end, json_codec());
	let diff_channel = diff_session.expose_keyed(store, |task: Task| task.key());
	print(i"diff-channel:{diff_channel}");
	let diff_client = ReactiveClient::new(diff_client_end, json_codec());
	let diff_mirror: KeyedSource<str, Task> = diff_client.keyed_source(diff_channel);
	let diff_lease = diff_mirror.sub(|_list| {});

	// (2) the CELL path — the same edits, said as ops.
	let cell: KeyedCell<str, Task> = KeyedCell::new(seed());
	let (cell_client_end, cell_server_end) = logged_pair("cell");
	let cell_session = ReactiveServer::new(cell_server_end, json_codec());
	let cell_channel = cell_session.expose_keyed_cell(cell);
	print(i"cell-channel:{cell_channel}");
	let cell_client = ReactiveClient::new(cell_client_end, json_codec());
	let cell_mirror: KeyedSource<str, Task> = cell_client.keyed_source(cell_channel);
	let cell_lease = cell_mirror.sub(|_list| {});

	// Edit, append, delete — the three A39 pinned for the keyed shape.
	store.update(|&mut list| { list[0] = Task { id = "a", label = "edited" }; });
	cell.update("a", |&mut task| { task.label = "edited"; });

	store.update(|&mut list| { list.push(Task { id = "c", label = "gamma" }); });
	cell.insert(Task { id = "c", label = "gamma" });

	store.update(|&mut list| { let _gone = list.remove(1); });
	cell.remove("b");

	// Two writes in ONE turn must coalesce into ONE patch on both paths.
	batch(|| {
		store.update(|&mut list| { list.push(Task { id = "d", label = "delta" }); });
		store.update(|&mut list| { list.push(Task { id = "e", label = "epsilon" }); });
	});
	batch(|| {
		cell.insert(Task { id = "d", label = "delta" });
		cell.insert(Task { id = "e", label = "epsilon" });
	});

	print(i"held diff={diff_mirror.get().unwrap_or([]).len()} cell={cell_mirror.get().unwrap_or([]).len()}");
	print(i"faults diff={diff_mirror.fault().is_some()} cell={cell_mirror.fault().is_some()}");
	diff_lease.dispose();
	cell_lease.dispose();
	batch(|| {});

	// (3) PER-KEY demand on both channels, through the same lifecycle the A39
	// pin walks: seed, follow, release, remount, and the key leaving.
	let diff_key = diff_mirror.sub_key("d", |value| match value {
		Some(let task) => print(i"diff-d:{task.label}"),
		None => print("diff-d:absent"),
	});
	let cell_key = cell_mirror.sub_key("d", |value| match value {
		Some(let task) => print(i"cell-d:{task.label}"),
		None => print("cell-d:absent"),
	});
	// `d` sits at index 2 of [a, c, d, e] — the diff path is told the change
	// positionally and the cell path by key, which is exactly the difference
	// under test, so the two must name the same element.
	store.update(|&mut list| { list[2] = Task { id = "d", label = "edited-d" }; });
	cell.update("d", |&mut task| { task.label = "edited-d"; });
	store.update(|&mut list| { let _gone = list.remove(2); });
	cell.remove("d");
	diff_key.dispose();
	cell_key.dispose();
	print(i"key-faults diff={diff_mirror.fault().is_some()} cell={cell_mirror.fault().is_some()}");
	print("done");
}
"#;

/// Split the relayed frames of one label out of the exhibit's stdout, with the
/// channel id — a process-wide counter, not a shape — normalized away.
fn relayed_frames(stdout: &str, label: &str, channel: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix(label))
        .map(|frame| frame.replace(&format!("[{channel},"), "[C,"))
        .collect()
}

fn labelled_value(stdout: &str, label: &str) -> String {
    stdout
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(label).map(str::to_string))
        .unwrap_or_else(|| panic!("`{label}` is missing from:\n{stdout}"))
}

#[test]
fn a_keyed_cell_patches_the_wire_exactly_as_the_diff_does() {
    let stdout = run_program("keyedcell", KEYED_CELL_AGAINST_THE_DIFF);
    let diff_channel = labelled_value(&stdout, "diff-channel:");
    let cell_channel = labelled_value(&stdout, "cell-channel:");
    assert_ne!(
        diff_channel, cell_channel,
        "the two channels must be distinct, or the comparison is vacuous"
    );
    let diff = relayed_frames(&stdout, "diff ", &diff_channel);
    let cell = relayed_frames(&stdout, "cell ", &cell_channel);
    assert!(
        !cell.is_empty(),
        "the cell's channel put nothing on the wire:\n{stdout}"
    );
    assert_eq!(
        cell, diff,
        "the cell exposure and the diff exposure must be the same channel, \
         frame for frame:\n{stdout}"
    );
    // And they are the frames A39 pinned for the keyed shape — ONE frame per
    // change, carrying only what moved, with two writes in one turn coalescing
    // into one patch of two ops. This half is the CONTROL: it is what the
    // `SignalCell<List<T>>` field still costs and still says.
    assert_eq!(
        diff,
        vec![
            "{\"Patch\":[C,[{\"Reset\":[{\"id\":\"a\",\"label\":\"alpha\"},{\"id\":\"b\",\"label\":\"beta\"}]}]]}",
            "{\"Patch\":[C,[{\"Update\":[\"a\",{\"id\":\"a\",\"label\":\"edited\"}]}]]}",
            "{\"Patch\":[C,[{\"Insert\":[\"c\",{\"id\":\"c\",\"label\":\"gamma\"},2]}]]}",
            "{\"Patch\":[C,[{\"Remove\":\"b\"}]]}",
            "{\"Patch\":[C,[{\"Insert\":[\"d\",{\"id\":\"d\",\"label\":\"delta\"},2]},{\"Insert\":[\"e\",{\"id\":\"e\",\"label\":\"epsilon\"},3]}]]}",
            // Per-key demand: the seed is an `Insert` (never a `Reset`), then
            // this key's changes and no other key's.
            "{\"Patch\":[C,[{\"Insert\":[\"d\",{\"id\":\"d\",\"label\":\"delta\"},2]}]]}",
            "{\"Patch\":[C,[{\"Update\":[\"d\",{\"id\":\"d\",\"label\":\"edited-d\"}]}]]}",
            "{\"Patch\":[C,[{\"Remove\":\"d\"}]]}",
        ],
        "the keyed frames moved:\n{stdout}"
    );
    assert!(
        stdout.contains("held diff=4 cell=4"),
        "both mirrors must hold the same collection:\n{stdout}"
    );
    assert!(
        stdout.contains("faults diff=false cell=false")
            && stdout.contains("key-faults diff=false cell=false"),
        "neither mirror may report a protocol fault:\n{stdout}"
    );
}

/// `[expose(keyed)] tasks: KeyedCell<str, Task>` — the macro form. A
/// `KeyedCell<K, T>` names BOTH types in its own arguments, so it is keyed by
/// its TYPE and needs nothing from the attribute (where a `SignalCell<List<T>>`
/// names only the element and A51 gave the key to `keyed = K`).
///
/// The claim is that the two field spellings are the SAME channel: the same
/// contract entry, therefore the same hash, and the same frames. The hash is
/// the load-bearing half — a service that swaps its field for a cell must not
/// break every deployed client — and it holds because the surface entry is
/// built from the written key and element, which both spellings supply.
const GENERATED_KEYED_CELL: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell };
import std::result::Result::{ self, Ok, Err };
import std::rpc::{
	DuplexEnd,
	KeyedCell,
	KeyedSource,
	ReactiveClient,
	ReactiveServer,
	RpcError,
	call,
	duplex_pair,
	local_rpc,
	register_session,
};
import std::wire::{ Frame, Keyed, Serializer, Wire };

[derive(Wire, PartialEq, Debug)]
struct Task { id: str, label: str }

impl Task with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

[service(CellStoreClient)]
struct CellStore {
	[expose(keyed)] tasks: KeyedCell<str, Task>,
}

impl CellStore {
	[rpc]
	fun touch(self): i32 {
		1
	}
}

[service(ListStoreClient)]
struct ListStore {
	[expose(keyed = str)] tasks: SignalCell<List<Task>>,
}

impl ListStore {
	[rpc]
	fun touch(self): i32 {
		1
	}
}

let cell: KeyedCell<str, Task> = KeyedCell::new([Task { id = "a", label = "alpha" }]);
let cell_store: CellStore = CellStore { tasks = cell };
let list: SignalCell<List<Task>> = Signal::new([Task { id = "a", label = "alpha" }]);
let list_store: ListStore = ListStore { tasks = list };

fun text_of(frame: Frame): str {
	match frame {
		Frame::Text(let text) => text,
		Frame::Binary(let _bytes) => "<binary>",
	}
}

fun logged_pair(label: str): (DuplexEnd, DuplexEnd) {
	let (client_end, client_relay) = duplex_pair();
	let (server_end, server_relay) = duplex_pair();
	client_relay.on_frame(|frame| server_relay.send(frame));
	server_relay.on_frame(|frame| {
		print(i"{label} {text_of(frame)}");
		client_relay.send(frame);
	});
	(client_end, server_end)
}

fun main() {
	print(i"cell-hash:{cell_store.contract_hash()}");
	print(i"list-hash:{list_store.contract_hash()}");

	let (client_end, server_end) = logged_pair("gen");
	register_session(1, server_end, json_codec());
	let transport = local_rpc(cell_store.dispatcher().into_protocol(json_codec()));
	let attached: Result<List<i32>, RpcError> = call(transport, json_codec(), "__attach", [|serializer: Serializer| 1.describe(serializer)]);
	let channels = attached.unwrap_or([]);
	print(i"gen-channel:{channels[0]}");
	let client = ReactiveClient::new(client_end, json_codec());
	let mirror: KeyedSource<str, Task> = client.keyed_source(channels[0]);
	let lease = mirror.sub(|_list| {});
	cell.insert(Task { id = "b", label = "beta" });
	cell.update("a", |&mut task| { task.label = "edited"; });
	cell.remove("a");
	print(i"held:{mirror.get().unwrap_or([]).len()}");
	print(i"fault:{mirror.fault().is_some()}");
	lease.dispose();
	print("done");
}
"#;

#[test]
fn a_generated_keyed_cell_field_is_the_list_form_s_channel_and_hash() {
    let stdout = run_program("keyedcellmacro", GENERATED_KEYED_CELL);
    let cell_hash = labelled_value(&stdout, "cell-hash:");
    let list_hash = labelled_value(&stdout, "list-hash:");
    assert_eq!(
        cell_hash, list_hash,
        "a `KeyedCell<K, T>` field and a `[expose(keyed = K)] SignalCell<List<T>>` \
         field are the same contract entry, so they must hash the same — a service \
         that swaps one for the other would otherwise break every deployed \
         client:\n{stdout}"
    );
    let channel = labelled_value(&stdout, "gen-channel:");
    assert_eq!(
        relayed_frames(&stdout, "gen ", &channel),
        vec![
            "{\"Patch\":[C,[{\"Reset\":[{\"id\":\"a\",\"label\":\"alpha\"}]}]]}",
            "{\"Patch\":[C,[{\"Insert\":[\"b\",{\"id\":\"b\",\"label\":\"beta\"},1]}]]}",
            "{\"Patch\":[C,[{\"Update\":[\"a\",{\"id\":\"a\",\"label\":\"edited\"}]}]]}",
            "{\"Patch\":[C,[{\"Remove\":\"a\"}]]}",
        ],
        "the generated cell exposure must patch element by element:\n{stdout}"
    );
    assert!(
        stdout.contains("held:1") && stdout.contains("fault:false"),
        "the mirror must follow the cell without a fault:\n{stdout}"
    );
}

/// What the cell actually removes, counted rather than timed: `Keyed::key`
/// calls per change.
///
/// `keyed_diff` re-keys BOTH snapshots and compares every retained element, so
/// the projection runs a fixed multiple of N times per change per connection;
/// the cell's writes carry the key already, so it runs ZERO times. Counting the
/// calls is the complexity claim stated exactly — no clock, no load, nothing to
/// be flaky about — and it is what the CPU measurement below is measuring.
const KEYED_COST_IN_KEY_CALLS: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell };
import std::rpc::{ KeyedCell, KeyedSource, ReactiveClient, ReactiveServer, duplex_pair };
import std::shared::Shared;
import std::wire::{ Keyed, Wire };

let key_calls: Shared<i32> = Shared::new(0);

[derive(Wire, PartialEq)]
struct Row { id: str, value: i32 }

impl Row with Keyed<str> {
	fun key(self): str {
		key_calls.write() = key_calls.read() + 1;
		self.id
	}
}

fun corpus(count: i32): List<Row> {
	mut all: List<Row> = [];
	mut index = 0;
	for index < count {
		all.push(Row { id = i"row-{index}", value = index });
		index += 1;
	}
	all
}

fun through_cell(rows: i32, changes: i32): i32 {
	let cell: KeyedCell<str, Row> = KeyedCell::new(corpus(rows));
	let (client_end, server_end) = duplex_pair();
	let session = ReactiveServer::new(server_end, json_codec());
	let client = ReactiveClient::new(client_end, json_codec());
	let channel = session.expose_keyed_cell(cell);
	let mirror: KeyedSource<str, Row> = client.keyed_source(channel);
	let lease = mirror.sub(|_list| {});
	// Zeroed AFTER the wiring: the corpus and the seed are setup, not change.
	key_calls.write() = 0;
	mut made = 0;
	for made < changes {
		cell.update("row-0", |&mut row| {
			row.value = made;
		});
		made += 1;
	}
	lease.dispose();
	key_calls.read()
}

fun through_diff(rows: i32, changes: i32): i32 {
	let store: SignalCell<List<Row>> = Signal::new(corpus(rows));
	let (client_end, server_end) = duplex_pair();
	let session = ReactiveServer::new(server_end, json_codec());
	let client = ReactiveClient::new(client_end, json_codec());
	let channel = session.expose_keyed(store, |row: Row| row.key());
	let mirror: KeyedSource<str, Row> = client.keyed_source(channel);
	let lease = mirror.sub(|_list| {});
	key_calls.write() = 0;
	mut made = 0;
	for made < changes {
		store.update(|&mut list| {
			list[0] = Row { id = "row-0", value = made };
		});
		made += 1;
	}
	lease.dispose();
	key_calls.read()
}

fun main() {
	let changes = 20;
	print(i"cell:200:{through_cell(200, changes)}");
	print(i"cell:2000:{through_cell(2000, changes)}");
	print(i"diff:200:{through_diff(200, changes)}");
	print(i"diff:2000:{through_diff(2000, changes)}");
	print("done");
}
"#;

#[test]
fn a_keyed_cell_re_keys_nothing_where_the_diff_re_keys_the_whole_collection() {
    let stdout = run_program("keyedcellwork", KEYED_COST_IN_KEY_CALLS);
    let count = |label: &str| -> i64 {
        labelled_value(&stdout, label)
            .parse()
            .unwrap_or_else(|_| panic!("`{label}` is not a count in:\n{stdout}"))
    };
    let (cell_small, cell_large) = (count("cell:200:"), count("cell:2000:"));
    let (diff_small, diff_large) = (count("diff:200:"), count("diff:2000:"));
    // The diff path is the CONTROL, and it must still be linear: ten times the
    // rows, ten times the projection calls. (If this ever stops holding, the
    // cell's number below has nothing to be compared against.)
    assert!(
        diff_small > 0 && diff_large >= diff_small * 8,
        "the diff path must re-key the collection on every change \
         (200 rows: {diff_small}, 2000 rows: {diff_large}):\n{stdout}"
    );
    // The cell path re-keys NOTHING: its writes carry the key already, and
    // both ends look it up by hash rather than by projection.
    assert_eq!(
        (cell_small, cell_large),
        (0, 0),
        "a `KeyedCell` change must not project a single element's key \
         (200 rows: {cell_small}, 2000 rows: {cell_large}):\n{stdout}"
    );
}

/// The cell twin of `KEYED_DIFF_COST` above: the same program, the same
/// parameters, the same single-element edit — said as `cell.update(key, ..)`
/// instead of as a whole-list write, so the exposure forwards the op the
/// mutation recorded instead of diffing two snapshots.
const KEYED_CELL_COST: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Subscription };
import std::rpc::{ KeyedCell, KeyedSource, ReactiveClient, ReactiveServer, duplex_pair };
import std::wire::{ Keyed, Wire };

[derive(Wire, PartialEq, Debug)]
struct Row {
	id: str,
	value: i32,
	body: str,
}

impl Row with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

fun corpus(count: i32): List<Row> {
	mut all: List<Row> = [];
	mut index = 0;
	for index < count {
		all.push(Row {
			id = i"row-{index}",
			value = index,
			body = "the quick brown fox jumps over the lazy dog, repeatedly and at length",
		});
		index += 1;
	}
	all
}

fun main() {
	let rows = __ROWS__;
	let connections = __CONNECTIONS__;
	let changes = __CHANGES__;
	let store: KeyedCell<str, Row> = KeyedCell::new(corpus(rows));

	mut leases: List<Subscription> = [];
	mut opened = 0;
	for opened < connections {
		let (client_end, server_end) = duplex_pair();
		let session = ReactiveServer::new(server_end, json_codec());
		let client = ReactiveClient::new(client_end, json_codec());
		let channel = session.expose_keyed_cell(store);
		let mirror: KeyedSource<str, Row> = client.keyed_source(channel);
		leases.push(mirror.sub(|_list| {}));
		opened += 1;
	}

	mut made = 0;
	for made < changes {
		// The same smallest change there is, said by key.
		store.update("row-0", |&mut row| {
			row.value = made;
		});
		made += 1;
	}

	for lease in leases {
		lease.dispose();
	}
	print(i"rows={rows} connections={connections} changes={changes}");
}
"#;

/// Build `template` with its three parameters substituted, run it under `node`,
/// and return the CHILD's CPU (user + system) for the run alone — the compile
/// is never inside a measured span. `repeats` runs of each shape, MINIMUM
/// taken: the minimum is the run that was least preempted, which is the right
/// estimator for a machine that is also doing something else.
fn measured_cpu(
    dir: &Path,
    template: &str,
    rows: i32,
    connections: i32,
    changes: i32,
    repeats: usize,
) -> Duration {
    write(
        dir,
        "src/main.vl",
        &template
            .replace("__ROWS__", &rows.to_string())
            .replace("__CONNECTIONS__", &connections.to_string())
            .replace("__CHANGES__", &changes.to_string()),
    );
    let built = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", dir.to_str().unwrap()])
        .output()
        .expect("build the measured program");
    assert!(
        built.status.success(),
        "the measured program did not build:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let bundle = dir.join("src").join("main.mjs");
    (0..repeats)
        .map(|_| {
            let before = children_cpu_now().expect("a children-CPU clock");
            let ran = Command::new("node")
                .arg(&bundle)
                .output()
                .expect("run the measured program");
            let after = children_cpu_now().expect("a children-CPU clock");
            assert!(
                ran.status.success(),
                "the measured program did not run:\n{}",
                String::from_utf8_lossy(&ran.stderr)
            );
            after.saturating_sub(before)
        })
        .min()
        .expect("at least one repeat")
}

/// A54's claim, as a GATE: the cell's cost per change does not grow with the
/// collection.
///
/// Pinned as a RATIO and never as a wall bound — the absolute number is a
/// property of the machine, the ratio is a property of the algorithm. Ten times
/// the rows must cost less than THREE times as much; measured at 1.32x
/// (0.0078 ms/change at 1,000 rows, 0.0103 at 10,000, one connection,
/// `getrusage(RUSAGE_CHILDREN)`, loadavg 14.8-17.3) and again at 1.10x on a box
/// at loadavg 45-50, against 12.1x for the diffing path on the same box in the
/// same minute (0.315 -> 3.814 ms); at eight connections the same step is
/// 0.299 -> 5.033 for the diff and 0.0048 -> 0.0148 for the cell. The bound
/// has 2.3x of headroom over the
/// measured slope and refuses the regression it exists for: restoring the
/// `get`/`set` pair `apply` used to copy the whole mirror through reads 4.52x.
///
/// The idle run subtracts the corpus build, the process start and the node
/// runtime, so what is left is the changes alone; two repeats of each shape and
/// the MINIMUM of them keeps a preempted run from being read as a slow one.
#[test]
fn a_keyed_cell_s_cost_per_change_does_not_grow_with_the_collection() {
    let Some(_probe) = children_cpu_now() else {
        eprintln!("PERF declined: this host exposes no children-CPU clock");
        return;
    };
    let dir = temp_project("keyed_cell_cost");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    // Enough changes that the measured span DOMINATES the idle baseline at
    // both sizes, which is the whole difficulty here: building the 10,000-row
    // corpus is 1-1.8 s of the idle run and varies by ~150 ms between runs, so
    // a measured span of the same order reads as noise — at 50,000 changes on
    // a box at loadavg 13 an unlucky pair once came out NEGATIVE, which the
    // non-degeneracy assertion below caught. 200,000 puts ~1.5 s of
    // attributable work against that baseline at 10,000 rows.
    const CHANGES: i32 = 200_000;
    const REPEATS: usize = 2;
    println!("PERF loadavg-before {}", loadavg_1m());
    let per_change = |rows: i32| -> f64 {
        let idle = measured_cpu(&dir, KEYED_CELL_COST, rows, 1, 0, REPEATS);
        let busy = measured_cpu(&dir, KEYED_CELL_COST, rows, 1, CHANGES, REPEATS);
        let attributable = busy.saturating_sub(idle);
        let per = attributable.as_secs_f64() * 1000.0 / f64::from(CHANGES);
        println!(
            "PERF {{\"section\":\"a54-keyed-cell\",\"rows\":{rows},\"connections\":1,\
             \"changes\":{CHANGES},\"idle_ms\":{:.1},\"busy_ms\":{:.1},\
             \"attributable_ms\":{:.1},\"ms_per_change\":{per:.5},\
             \"clock\":\"children-cpu\",\"load\":\"{}\"}}",
            idle.as_secs_f64() * 1000.0,
            busy.as_secs_f64() * 1000.0,
            attributable.as_secs_f64() * 1000.0,
            loadavg_1m()
        );
        per
    };
    let small = per_change(1_000);
    let large = per_change(10_000);
    println!("PERF loadavg-after {}", loadavg_1m());
    let _ = std::fs::remove_dir_all(&dir);
    // Non-degenerate: a measurement that read zero (or negative) work would
    // pass any ratio bound while measuring nothing.
    assert!(
        small > 0.0 && large > 0.0,
        "the measurement read no attributable work at all \
         (1,000 rows: {small} ms/change, 10,000 rows: {large} ms/change) — \
         raise CHANGES or run it on a quieter machine"
    );
    let ratio = large / small;
    assert!(
        ratio < 3.0,
        "a `KeyedCell` change must not cost more as the collection grows: \
         ten times the rows cost {ratio:.2}x (1,000 rows: {small:.5} ms/change, \
         10,000 rows: {large:.5}). The diffing exposure this replaces is 12x \
         over the same step; anything near that means the op path is not being \
         taken."
    );
}

/// The reporting harness beside `keyed_diff_cpu_per_change_per_connection`:
/// both exposures, both scales, both connection counts, one run each of the
/// changing and unchanging shape so the seed and the runtime subtract out.
/// `#[ignore]`d for its sibling's reason — it is a measurement, not a gate, and
/// it spawns sixteen node processes. Run it with:
///
/// ```text
/// cargo nextest run -p vilan-cli --test reactive_channels --run-ignored \
///     ignored-only -E 'test(keyed_cell_against)' --no-capture
/// ```
#[test]
#[ignore = "A54: a measurement, not a gate — it spawns sixteen node processes and reports numbers"]
fn keyed_cell_against_the_diff_cpu_per_change_per_connection() {
    let Some(_probe) = children_cpu_now() else {
        eprintln!("PERF declined: this host exposes no children-CPU clock");
        return;
    };
    let dir = temp_project("keyed_cell_report");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    println!("PERF loadavg-before {}", loadavg_1m());
    // The two paths need different change counts to clear the same baseline:
    // the diff is milliseconds per change and the cell is microseconds.
    for (section, template, changes) in [
        ("a51-keyed-diff", KEYED_DIFF_COST, 2_000),
        ("a54-keyed-cell", KEYED_CELL_COST, 50_000),
    ] {
        for rows in [1_000, 10_000] {
            for connections in [1, 8] {
                let idle = measured_cpu(&dir, template, rows, connections, 0, 1);
                let busy = measured_cpu(&dir, template, rows, connections, changes, 1);
                let attributable = busy.saturating_sub(idle);
                let per = attributable.as_secs_f64() * 1000.0
                    / f64::from(changes)
                    / f64::from(connections);
                println!(
                    "PERF {{\"section\":\"{section}\",\"rows\":{rows},\
                     \"connections\":{connections},\"changes\":{changes},\
                     \"idle_ms\":{:.1},\"busy_ms\":{:.1},\"attributable_ms\":{:.1},\
                     \"ms_per_change_per_connection\":{per:.5},\
                     \"clock\":\"children-cpu\",\"load\":\"{}\"}}",
                    idle.as_secs_f64() * 1000.0,
                    busy.as_secs_f64() * 1000.0,
                    attributable.as_secs_f64() * 1000.0,
                    loadavg_1m()
                );
            }
        }
    }
    println!("PERF loadavg-after {}", loadavg_1m());
    let _ = std::fs::remove_dir_all(&dir);
}
