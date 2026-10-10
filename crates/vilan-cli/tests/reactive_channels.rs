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
	let length = match frame {
		Frame::Text(let value) => value.len(),
		Frame::Binary(let bytes) => bytes.len(),
	};
	length.as_i32()
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
		Some(let list) => list.len().as_i32(),
		None => -1,
	}
}

/// The lazy attach, generically: `on_change` is the trait's primitive, so a
/// generic consumer reaches the mirror's counted one.
fun watch<S: Source<Option<List<Row>>>>(source: S, seen: SignalCell<i32>): Subscription {
	source.on_change(|value| match value {
		Some(let list) => seen.set(list.len().as_i32()),
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
		print(i"or-len:{rendered.sample().len()}");
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
	let attached: Result<List<i32>, RpcError> = call(transport, json_codec(), "__attach", [|mut serializer: Serializer| 1.describe(&mut serializer)]);
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

mod support;

fn temp_project(tag: &str) -> PathBuf {
    let dir = support::scratch_root().join(format!(
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
    let (stdout, stderr) = run_program_capturing(tag, source);
    assert!(
        stderr.trim().is_empty(),
        "the program wrote to stderr:\n{stderr}\n--- stdout ---\n{stdout}"
    );
    stdout
}

/// [`run_program`] for a program that is EXPECTED to build with one warning:
/// its stderr must carry `warning` and nothing that reads as an error. The
/// A135 programs are the shape the warning exists for, so the build that runs
/// them is also the pin that it is raised through the real pipeline.
fn run_program_warning(tag: &str, source: &str, warning: &str) -> String {
    let (stdout, stderr) = run_program_capturing(tag, source);
    assert!(
        stderr.contains(warning) && !stderr.contains("Error"),
        "expected exactly the warning {warning:?} on stderr, got:\n{stderr}\n--- stdout ---\n{stdout}"
    );
    stdout
}

fn run_program_capturing(tag: &str, source: &str) -> (String, String) {
    run_package_capturing(tag, &[("src/main.vl", source)])
}

/// [`run_program`] over a PACKAGE: `files` (path relative to the project root →
/// contents) beside the manifest, `src/main.vl` among them — for the pins whose
/// claim is about a service declared in a module the entry imports (B519).
fn run_package(tag: &str, files: &[(&str, &str)]) -> String {
    let (stdout, stderr) = run_package_capturing(tag, files);
    assert!(
        stderr.trim().is_empty(),
        "the program wrote to stderr:\n{stderr}\n--- stdout ---\n{stdout}"
    );
    stdout
}

fn run_package_capturing(tag: &str, files: &[(&str, &str)]) -> (String, String) {
    let dir = temp_project(tag);
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    for (relative, contents) in files {
        write(&dir, relative, contents);
    }
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
    let _ = std::fs::remove_dir_all(&dir);
    (stdout, stderr)
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
            "status:Status::Waiting",
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
	let length = match frame {
		Frame::Text(let value) => value.len(),
		Frame::Binary(let bytes) => bytes.len(),
	};
	length.as_i32()
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
	let one_channel = store.derive(|all: List<Message>| all.filter(|message| message.channel == 0)).memo();

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
	let length = match frame {
		Frame::Text(let value) => value.len(),
		Frame::Binary(let bytes) => bytes.len(),
	};
	length.as_i32()
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
            "status:Status::Ready",
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
	wire_end.send(encode_patch(codec, channel, |mut serializer: Serializer| {
		serializer.begin_list(stray.len().as_i32());
		for op in stray {
			op.describe(&mut serializer);
		}
		serializer.end_list();
	}));
	print(i"fault-after-update:{mirror.fault().unwrap_or("none")}");
	print(i"held:{render(mirror.get().unwrap_or([]))}");

	// Sticky: the first fault is the one kept.
	let second: List<Delta<str, Row>> = [Delta::Remove("yyy")];
	wire_end.send(encode_patch(codec, channel, |mut serializer: Serializer| {
		serializer.begin_list(second.len().as_i32());
		for op in second {
			op.describe(&mut serializer);
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
		self.tasks.get().len().as_i32()
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
	let attached: Result<List<i32>, RpcError> = call(transport, json_codec(), "__attach", [|mut serializer: Serializer| 1.describe(&mut serializer)]);
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

// --- §9.2 / R3: the DYNAMIC mirror, in process and frame by frame -----------
//
// The generated end of this lives in `service_layer.rs` over a real socket
// (`a_handle_returning_method…`, `a_hundred_handles…`). What belongs HERE is
// what a socket can only blur: the exact frames a dispose-and-remount costs,
// which is R3's whole subject. `Origin` and `ReactiveClient::minted_source`
// are the public seam the generated stub calls, so a hand-written origin over
// a `duplex_pair` exercises the same code the macro emits.

/// A dispose and a remount, inside one turn and then across two — the frame
/// count is the whole measurement, so the link counts frames in both
/// directions rather than bytes.
const MINTED_REMOUNT: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ FlushPolicy, Signal, SignalCell, turn };
import std::result::Result::{ self, Ok };
import std::rpc::{ DuplexEnd, Origin, ReactiveClient, ReactiveServer, RemoteSource, RpcError, duplex_pair };
import std::shared::Shared;
import std::time::{ Duration, sleep_for };
import std::wire::{ Serializer, Wire };

/// A duplex pair with both legs counted, one FRAME at a time.
fun counted_link(up: Shared<i32>, down: Shared<i32>): (DuplexEnd, DuplexEnd) {
	let (client_end, client_relay) = duplex_pair();
	let (server_end, server_relay) = duplex_pair();
	client_relay.on_frame(|frame| {
		up.write() = up.read() + 1;
		server_relay.send(frame);
	});
	server_relay.on_frame(|frame| {
		down.write() = down.read() + 1;
		client_relay.send(frame);
	});
	(client_end, server_end)
}

fun main() {
	let up: Shared<i32> = Shared::new(0);
	let down: Shared<i32> = Shared::new(0);
	let (client_end, server_end) = counted_link(up, down);
	let session = ReactiveServer::new(server_end, json_codec());
	let client = ReactiveClient::new(client_end, json_codec());
	let cell: SignalCell<i32> = Signal::new(1);
	let mints: Shared<i32> = Shared::new(0);

	// The origin a generated stub would carry: the call that minted the
	// channel, and the seam that issues it again. In process, "issuing" it is
	// exposing the same source afresh — which is exactly what the server route
	// does for a real one.
	let origin = Origin {
		method = "get_message",
		describers = [],
		reissue = |name: str, args: List<|Serializer| void>| {
			mints.write() = mints.read() + 1;
			// Three-valued since A92: `Ok(None)` is the server saying there
			// is no such source, which this seam never says.
			let fresh: Result<Option<i32>, RpcError> = Ok(Some(session.expose_dynamic(cell)));
			fresh
		},
	};
	mints.write() = mints.read() + 1;
	let mirror: RemoteSource<i32> = client.minted_source(session.expose_dynamic(cell), origin);

	// Lazy: a minted handle nothing watches has sent nothing at all.
	print(i"minted:up={up.read()} down={down.read()} sources={session.sources.read().len()}");
	let held = mirror.sub(|value| print(i"held:{value}"));
	print(i"leased:up={up.read()} down={down.read()} live={session.live.read().len()}");

	// (c) A dispose and a rebuild inside ONE turn: the close never leaves the
	// turn, `acquire` cancels it, and nothing crosses.
	up.write() = 0;
	down.write() = 0;
	turn(FlushPolicy::AtEnd, || {
		held.dispose();
		let refreshed = mirror.sub(|value| print(i"same-turn:{value}"));
	});
	sleep_for(Duration::millis(0));
	print(i"same-turn:up={up.read()} down={down.read()} sources={session.sources.read().len()}");

	// (d) A dispose in one turn and a mount in ANOTHER — two event handlers, a
	// route change, a `each` rebuilding rows. Different turns, one
	// MACROTASK: the settle is the first look and the microtask hop is the
	// second, so this costs nothing either.
	up.write() = 0;
	down.write() = 0;
	turn(FlushPolicy::AtEnd, || {
		mirror.release();
	});
	turn(FlushPolicy::AtEnd, || {
		mirror.acquire();
	});
	sleep_for(Duration::millis(0));
	print(i"cross-turn:up={up.read()} down={down.read()} sources={session.sources.read().len()} mints={mints.read()}");

	// And the close that IS real: past the settle and past the hop, the
	// `Unsubscribe` goes out and the server revokes the dynamic channel.
	up.write() = 0;
	down.write() = 0;
	mirror.release();
	sleep_for(Duration::millis(0));
	print(i"closed:up={up.read()} down={down.read()} sources={session.sources.read().len()}");

	// Demand returns: the mirror re-issues its origin and rebinds.
	cell.set(7);
	let again = mirror.sub(|value| print(i"again:{value}"));
	sleep_for(Duration::millis(0));
	print(i"re-minted:sources={session.sources.read().len()} live={session.live.read().len()} mints={mints.read()}");
	again.dispose();
	sleep_for(Duration::millis(0));
	print("done");
}
"#;

/// R3, point 1 — the microtask second look — measured in frames.
///
/// The shipped rule is that a 1→0 defers its `Unsubscribe` to the ambient
/// turn's settle, so a dispose and a rebuild INSIDE one turn churn nothing.
/// That covers a `each` row refreshing and nothing else: a dispose in one
/// event handler and a mount in the next are two turns, and the settle of the
/// first has already fired by the time the second runs. Both are one
/// MACROTASK, and R3 makes the whole macrotask free by taking one microtask
/// hop before the frame goes out.
///
/// It is the DYNAMIC mirror's hop and not every mirror's, and the asymmetry
/// is the reason: on a dynamic channel the server revokes on `Unsubscribe`,
/// so a spurious one costs a whole re-mint round trip — a call, a fresh
/// capability, a fresh `Subscribe`, a fresh seed — where a field channel would
/// only re-`Subscribe`, and would pay for the hop with the promptness
/// `remote-sources.md` §2 ratified (four A25 pins in `vilan-core` record it: a
/// write after the last dispose puts nothing on the wire).
///
/// **Red without the hop, and redder than expected.** With `flush_close`
/// sending inline (its shipped body), the plant reads
/// `same-turn:up=2 down=1 sources=1` and
/// `cross-turn:up=2 down=1 sources=1 mints=3` — an `Unsubscribe` and a
/// re-`Subscribe` up, a seed down, and a re-mint, in BOTH cases. The
/// cross-turn red is R3's own subject. The same-TURN red is the finding
/// underneath it: `release` runs from the subscription's stored release hook,
/// so the turn `at_settle` reads is the one that closure captured AT CREATION
/// — none, for a lease taken outside a turn — and the settle-deferral never
/// applied to it at all. The hop is what makes the shipped same-turn promise
/// true for a lease whose subscription was born anywhere.
#[test]
fn a_minted_mirror_remounting_inside_one_macrotask_sends_nothing() {
    let stdout = run_program("mintedremount", MINTED_REMOUNT);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            // Minted and unwatched: a capability exists, and not one frame has
            // crossed in either direction.
            "minted:up=0 down=0 sources=1",
            // The lease is one `Subscribe` up and one seeding `Update` down.
            "held:1",
            "leased:up=1 down=1 live=1",
            // Same turn: the re-`sub` fires immediately from the cache — a
            // local read, not a frame.
            "same-turn:1",
            "same-turn:up=0 down=0 sources=1",
            // Different turns, one macrotask: still nothing, and the
            // capability is untouched, so no re-mint was needed.
            "cross-turn:up=0 down=0 sources=1 mints=1",
            // The real close: one `Unsubscribe` up, and the dynamic
            // capability is withdrawn by it.
            "closed:up=1 down=0 sources=0",
            // Demand returns. The new observer fires at once from the CACHE
            // — `again:1`, the value the mirror was last told, painted before
            // the round trip rather than a `Waiting` for its duration — and
            // the re-issued origin's seed then carries the value as of NOW to
            // every observer the mirror has, the earlier `same-turn` lease
            // included.
            "again:1",
            "same-turn:7",
            "again:7",
            "re-minted:sources=1 live=1 mints=2",
            "done",
        ],
        "the minted mirror's remount economy went differently:\n{stdout}"
    );
}

/// The owner hook (R3, point 4) and the reconnect replay (§9.2's `origin`),
/// both in process.
const MINTED_OWNER_AND_REPLAY: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell, comp };
import std::result::Result::{ self, Ok };
import std::rpc::{ Origin, ReactiveClient, ReactiveServer, RemoteSource, RpcError, duplex_pair };
import std::shared::Shared;
import std::time::{ Duration, sleep_for };
import std::wire::{ Serializer, Wire };

fun main() {
	let (app_end, wire_end) = duplex_pair();
	let cell: SignalCell<str> = Signal::new("first");
	// Distinct SOURCES, because a channel is per source now: `expose_dynamic`
	// dedups by the cell's identity (A92), so three exposures of one cell
	// would be one channel and this pin counts three.
	let spare: SignalCell<str> = Signal::new("spare");
	let unbound: SignalCell<str> = Signal::new("unbound");
	let session: Shared<ReactiveServer> = Shared::new(ReactiveServer::new(wire_end, json_codec()));
	let client = ReactiveClient::new(app_end, json_codec());
	let mints: Shared<i32> = Shared::new(0);

	// One origin, three mirrors minted through it — the seam re-exposes on
	// whichever session is live, which is what a reconnect makes true.
	let origin = Origin {
		method = "get_message",
		describers = [],
		reissue = |name: str, args: List<|Serializer| void>| {
			mints.write() = mints.read() + 1;
			let fresh: Result<Option<i32>, RpcError> = Ok(Some(session.read().expose_dynamic(cell)));
			fresh
		},
	};

	// (f) THE OWNER HOOK. A mirror minted inside an owner scope that nothing
	// ever leased: its owner's disposal is the only event that can free the
	// capability, and it does.
	let (orphan, scope) = comp(|| {
		let inner: RemoteSource<str> = client.minted_source(session.read().expose_dynamic(cell), origin);
		inner
	});
	print(i"under-owner:sources={session.read().sources.read().len()}");
	scope.dispose();
	print(i"owner-disposed:sources={session.read().sources.read().len()}");

	// A mirror minted with NO ambient owner lives with the connection — the
	// documented case, and the reason the read is owner-optional.
	let ownerless: RemoteSource<str> = client.minted_source(session.read().expose_dynamic(spare), origin);
	print(i"ownerless:sources={session.read().sources.read().len()}");

	// (g) THE RECONNECT REPLAY. One WATCHED minted mirror, one unwatched, and
	// one hand-wired mirror with no origin at all.
	let watched: RemoteSource<str> = client.minted_source(session.read().expose_dynamic(cell), origin);
	let lease = watched.sub(|value| print(i"watched:{value}"));
	let hand: RemoteSource<str> = client.source(session.read().expose_dynamic(unbound));
	print(i"before-drop:sources={session.read().sources.read().len()} mints={mints.read()}");

	// The connection is replaced: the old session dies with every channel it
	// ever minted, and a fresh one takes the wire.
	session.read().dispose();
	session.write() = ReactiveServer::new(wire_end, json_codec());
	cell.set("second");
	print(i"while-down:{watched.get().unwrap_or("?")}");

	// What `reattach_mirrors` runs after the positional `__attach` rebind.
	client.replay_dynamic();
	sleep_for(Duration::millis(0));
	print(i"replayed:sources={session.read().sources.read().len()} live={session.read().live.read().len()} mints={mints.read()}");
	print(i"after:{watched.get().unwrap_or("?")}");
	cell.set("third");
	print(i"following:{watched.get().unwrap_or("?")}");

	// The origin-less mirror is nobody's to replay, and `invalidate_dynamic`
	// is still its answer.
	client.invalidate_dynamic();
	print(i"hand:{hand.status().get().debug()}");
	lease.dispose();
	sleep_for(Duration::millis(0));
	print("done");
}
"#;

/// The two lifetimes a dynamic mirror can end with that its own lease cannot
/// decide: an OWNER disposing, and a CONNECTION being replaced.
///
/// **The owner hook (R3, point 4).** A handle a call handed out and nothing
/// ever leased has no `Unsubscribe` owed — the lease machinery never ran — so
/// without this it holds a server capability until the socket closes. Minted
/// under an ambient owner, that owner's disposal is the honest end of its
/// life. The read is owner-OPTIONAL (`defer_to_owner`, `register_with_owner`'s
/// shape), so the ownerless case is not refused: a handle fetched at the top
/// of `main` lives with the connection, which frees every channel it ever
/// minted in one act, and that is documented rather than diagnosed.
///
/// **The replay (§9.2's `origin`).** A41 recorded that a mirror minted from a
/// runtime channel id could not be rebound after a reconnect, "because the
/// fresh session never minted it, and no protocol form exists yet to ask for
/// it again". The origin IS that form: the call and its arguments, re-issued
/// after the fresh session is attached. Only a WATCHED mirror is replayed —
/// an unwatched one would cost a call nobody asked for, and its next lease
/// re-mints anyway — and a mirror with NO origin (the hand-wired
/// `ReactiveClient::source`) is left to `invalidate_dynamic`, which is what
/// "retired for generated mirrors, kept for origin-less ones" means.
///
/// The three mirrors are over three distinct SOURCES on purpose (A92): a
/// dynamic channel is per source now, so exposing one cell three times would
/// answer one channel three times and this pin would be counting dedup rather
/// than lifetimes. Dedup has its own pin (`service_layer`'s demand test).
#[test]
fn an_owner_and_a_reconnect_each_end_a_minted_mirror_the_lease_cannot() {
    let stdout = run_program("mintedowner", MINTED_OWNER_AND_REPLAY);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "under-owner:sources=1",
            // The owner disposed a mirror nothing had leased, and the
            // capability went with it.
            "owner-disposed:sources=0",
            // No ambient owner: the mirror lives with the connection.
            "ownerless:sources=1",
            "watched:first",
            // Three capabilities on the doomed session: the ownerless one,
            // the watched one, and the hand-wired one. `mints` counts REPLAYS
            // — the initial exposures went through `expose_dynamic` directly,
            // exactly as a server route does.
            "before-drop:sources=3 mints=0",
            // The session is gone; the mirror still holds what it was last
            // told.
            "while-down:first",
            // The replay re-issues exactly the WATCHED mirror's origin — one
            // call, not three — and the mirror rebinds onto the fresh
            // session's channel and re-subscribes, so the seed lands before
            // the replay call has even returned.
            "watched:second",
            "replayed:sources=1 live=1 mints=1",
            "after:second",
            "watched:third",
            "following:third",
            // The origin-less mirror was not replayed, and invalidation is
            // still what says so.
            "hand:Status::Waiting",
            "done",
        ],
        "the owner hook or the replay went differently:\n{stdout}"
    );
}

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

/// B291's network face: a lease taken from a continuation whose owner already
/// died must not leave the channel open.
const LATE_MIRROR_LEASE: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Disposable, Owner, Signal, SignalCell, owner_scope };
import std::rpc::{ DuplexEnd, ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };
import std::shared::Shared;
import std::time::{ Duration, sleep_for };

fun counted_link(up: Shared<i32>, down: Shared<i32>): (DuplexEnd, DuplexEnd) {
	let (client_end, client_relay) = duplex_pair();
	let (server_end, server_relay) = duplex_pair();
	client_relay.on_frame(|frame| {
		up.write() = up.read() + 1;
		server_relay.send(frame);
	});
	server_relay.on_frame(|frame| {
		down.write() = down.read() + 1;
		client_relay.send(frame);
	});
	(client_end, server_end)
}

async fun main() {
	let up: Shared<i32> = Shared::new(0);
	let down: Shared<i32> = Shared::new(0);
	let (client_end, server_end) = counted_link(up, down);
	let session = ReactiveServer::new(server_end, json_codec());
	let client = ReactiveClient::new(client_end, json_codec());
	let cell: SignalCell<i32> = Signal::new(1);
	let mirror: RemoteSource<i32> = client.source(session.expose(cell));

	// The CONTROL: a lease taken under a live owner opens the channel, and the
	// owner's disposal closes it. Unchanged by B291.
	let control = Owner::new();
	owner_scope.run(control, || {
		mirror.effect(|value| print(i"control:{value.unwrap_or(-1)}"));
	});
	print(i"control-held:up={up.read()} down={down.read()} live={session.live.read().len()}");
	control.dispose();
	sleep_for(Duration::millis(0));
	print(i"control-gone:live={session.live.read().len()}");

	// The late one: the continuation captured its owner at CREATION, and that
	// owner was disposed before the continuation ever ran — kolt's channel
	// switch before the channel's first reply.
	up.write() = 0;
	down.write() = 0;
	let owner = Owner::new();
	owner_scope.run(owner, || {
		async {
			let _tick: i32 = await async 1;
			mirror.effect(|value| print(i"late:{value.unwrap_or(-1)}"));
		};
	});
	owner.dispose();
	let _first: i32 = await async 1;
	let _second: i32 = await async 1;
	sleep_for(Duration::millis(0));
	print(i"late-lease:up={up.read()} down={down.read()} live={session.live.read().len()}");
	up.write() = 0;
	down.write() = 0;
	cell.set(2);
	print(i"after-change:up={up.read()} down={down.read()}");
	print("done");
}
"#;

#[test]
fn b291_a_lease_taken_after_its_owner_died_leaves_the_server_forwarding_nothing() {
    // The measured choice (B291's second half). Two shapes were available for
    // the mirror: SUBSCRIBE-AND-RELEASE IN ONE SEGMENT — the general
    // `Owner::take` fix alone, nothing in `rpc.vl` — which costs `up=2 down=1`
    // (a `Subscribe`, its seeding `Update`, the `Unsubscribe` at the settle),
    // fires the observer once with the seed exactly as a local `effect` does,
    // and leaves `live=0`; or ACQUIRE-NOTHING — `effect`/`map` reading
    // `Owner::is_disposed` before leasing — which costs `up=0 down=0` and
    // leaves `live=0` too, but never calls the observer and answers only the
    // two ambient-owner call sites. The first is built: it is the general fix,
    // it is the backstop the second needs anyway (a hand-written
    // `owner.take(mirror.sub(..))` and every generated client go through
    // `take`, not through `effect`), and it keeps a mirror's `effect`
    // indistinguishable from a local source's. The three frames are a one-off
    // at a torn-down boundary, and `after-change` is what they buy.
    let stdout = run_program("latemirror", LATE_MIRROR_LEASE);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "control:1",
            "control-held:up=1 down=1 live=1",
            "control-gone:live=0",
            // The observer's one immediate call still happens — `effect` is
            // `effect_on_change` plus that call — and it is the only one.
            "late:1",
            "late-lease:up=2 down=1 live=0",
            // Before B291 the lease was parked on a dead owner's list, so the
            // server forwarded this write (`up=0 down=1`) and every one after
            // it for the rest of the session.
            "after-change:up=0 down=0",
            "done",
        ],
        "the late mirror lease went differently:\n{stdout}"
    );
}

/// B283: a lease taken OUTSIDE every turn and disposed-and-rebuilt inside one.
const OUTSIDE_IN_LEASE: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Disposable, FlushPolicy, Signal, SignalCell, turn };
import std::rpc::{ DuplexEnd, ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };
import std::shared::Shared;
import std::time::{ Duration, sleep_for };

fun counted_link(up: Shared<i32>, down: Shared<i32>): (DuplexEnd, DuplexEnd) {
	let (client_end, client_relay) = duplex_pair();
	let (server_end, server_relay) = duplex_pair();
	client_relay.on_frame(|frame| {
		up.write() = up.read() + 1;
		server_relay.send(frame);
	});
	server_relay.on_frame(|frame| {
		down.write() = down.read() + 1;
		client_relay.send(frame);
	});
	(client_end, server_end)
}

fun main() {
	let up: Shared<i32> = Shared::new(0);
	let down: Shared<i32> = Shared::new(0);
	let (client_end, server_end) = counted_link(up, down);
	let session = ReactiveServer::new(server_end, json_codec());
	let client = ReactiveClient::new(client_end, json_codec());
	let cell: SignalCell<i32> = Signal::new(1);
	// A HAND-WIRED mirror, deliberately: it has no origin, so `flush_close`
	// sends at the settle with no microtask hop behind it. The hop is what
	// hides this defect on a dynamic mirror; this measures the settle itself.
	let mirror: RemoteSource<i32> = client.source(session.expose(cell));

	// The lease is taken outside every turn — the top of `main`, a
	// module-level binding, a handle fetched before the first event.
	let outside = mirror.sub(|value| print(i"outside:{value}"));
	print(i"leased:up={up.read()} down={down.read()} live={session.live.read().len()}");

	// ... and disposed-and-rebuilt INSIDE one, which the guide promises churns
	// nothing.
	up.write() = 0;
	down.write() = 0;
	turn(FlushPolicy::AtEnd, || {
		outside.dispose();
		let refreshed = mirror.sub(|value| print(i"same-turn:{value}"));
	});
	sleep_for(Duration::millis(0));
	print(i"same-turn:up={up.read()} down={down.read()} live={session.live.read().len()}");

	// The CONTROL: a lease born INSIDE a turn and rebuilt inside another has
	// always been free, and must stay so.
	up.write() = 0;
	down.write() = 0;
	mut inside_handle = turn(FlushPolicy::AtEnd, || mirror.sub(|value| print(i"inside:{value}")));
	turn(FlushPolicy::AtEnd, || {
		inside_handle.dispose();
		let again = mirror.sub(|value| print(i"inside-again:{value}"));
	});
	sleep_for(Duration::millis(0));
	print(i"control:up={up.read()} down={down.read()} live={session.live.read().len()}");
	print("done");
}
"#;

#[test]
fn b283_a_lease_born_outside_a_turn_still_rebuilds_inside_one_for_free() {
    // B283. `release` is reached through the subscription's stored release
    // hook, and a stored closure reads the turn it captured AT CREATION (spec
    // §8.4's context rule). A lease born outside every turn therefore deferred
    // its `Unsubscribe` against NO turn, sent it inline, and the re-`sub` a
    // line later re-opened the channel: `same-turn:up=2 down=1` for a rebuild
    // the guide promises churns nothing. The promise held only for a
    // subscription that happened to be born inside the turn it is rebuilt in —
    // the `control` line, which was already free and still is.
    //
    // The fix is the narrow one: `Subscription::dispose` publishes the turn
    // ambient at the RELEASE (`releasing_turns`), and the mirror's release path
    // says `at_release_settle` instead of `at_settle`. `at_settle`'s own rule
    // is untouched, which is what keeps the four A25 markdown pins and the
    // captured-at-creation rule for a `set` from a stored callback intact.
    let stdout = run_program("outsidein", OUTSIDE_IN_LEASE);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "outside:1",
            "leased:up=1 down=1 live=1",
            "same-turn:1",
            // Was `up=2 down=1`.
            "same-turn:up=0 down=0 live=1",
            "inside:1",
            "inside-again:1",
            "control:up=0 down=0 live=1",
            "done",
        ],
        "the outside-in lease's same-turn rebuild went differently:\n{stdout}"
    );
}

/// A92's mint, in process: the unleased mirror, the three things its first
/// lease can be told, and the join.
const UNLEASED_MINT: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell };
import std::result::Result::{ self, Ok, Err };
import std::rpc::{ Origin, ReactiveClient, ReactiveServer, RemoteSource, RpcError, Status, duplex_pair };
import std::shared::Shared;
import std::time::{ Duration, sleep_for };
import std::wire::{ Serializer, Wire };

fun main() {
	let (app_end, wire_end) = duplex_pair();
	let cell: SignalCell<str> = Signal::new("first");
	let session = ReactiveServer::new(wire_end, json_codec());
	let client = ReactiveClient::new(app_end, json_codec());
	let asks: Shared<i32> = Shared::new(0);
	// What the next ask is answered with — the three things a minting call can
	// say, driven from the test rather than from a server's mood.
	let answer: Shared<str> = Shared::new("fail");

	let origin = Origin {
		method = "get_note",
		describers = [],
		reissue = |name: str, args: List<|Serializer| void>| {
			asks.write() = asks.read() + 1;
			let told = answer.read();
			if told == "fail" {
				let failed: Result<Option<i32>, RpcError> = Err(RpcError::Remote("no route"));
				failed
			} else if told == "absent" {
				let absent: Result<Option<i32>, RpcError> = Ok(None);
				absent
			} else {
				let fresh: Result<Option<i32>, RpcError> = Ok(Some(session.expose_dynamic(cell)));
				fresh
			}
		},
	};

	// UNLEASED: no call, no capability, no frame, and `Waiting` because
	// nothing has been asked.
	let mirror: RemoteSource<str> = client.unleased_source(origin);
	print(i"minted:asks={asks.read()} sources={session.sources.read().len()} status={mirror.status().get().debug()}");

	// The first lease IS the mint. This one fails, and the mirror says so.
	let first = mirror.sub(|value| print(i"value:{value}"));
	sleep_for(Duration::millis(0));
	print(i"failed:asks={asks.read()} sources={session.sources.read().len()} status={mirror.status().get().debug()}");
	first.dispose();
	sleep_for(Duration::millis(0));
	sleep_for(Duration::millis(0));

	// The next 0→1 asks again — and is told there is no such source.
	answer.write() = "absent";
	let second = mirror.sub(|value| print(i"value:{value}"));
	sleep_for(Duration::millis(0));
	print(i"absent:asks={asks.read()} sources={session.sources.read().len()} status={mirror.status().get().debug()} held={mirror.get().is_some()}");
	second.dispose();
	sleep_for(Duration::millis(0));
	sleep_for(Duration::millis(0));

	// And the retry that lands: a channel, a seed, `Ready`.
	answer.write() = "ok";
	let third = mirror.sub(|value| print(i"value:{value}"));
	sleep_for(Duration::millis(0));
	print(i"ready:asks={asks.read()} sources={session.sources.read().len()} status={mirror.status().get().debug()} held={mirror.get().unwrap_or("?")}");
	third.dispose();
	sleep_for(Duration::millis(0));
	sleep_for(Duration::millis(0));
	print(i"closed:sources={session.sources.read().len()}");

	// THE JOIN. Two leases taken on an unleased mirror before the mint has
	// landed are one call, not two: `remint` clears `released` before it
	// awaits, and the rebind subscribes for whatever demand exists by then.
	let twin: RemoteSource<str> = client.unleased_source(origin);
	let left = twin.sub(|value| print(i"left:{value}"));
	let right = twin.sub(|value| print(i"right:{value}"));
	print(i"in-flight:asks={asks.read()}");
	sleep_for(Duration::millis(0));
	print(i"joined:asks={asks.read()} sources={session.sources.read().len()} live={session.live.read().len()}");
	left.dispose();
	right.dispose();
	sleep_for(Duration::millis(0));
	sleep_for(Duration::millis(0));
	print(i"parted:sources={session.sources.read().len()} live={session.live.read().len()}");
	print("done");
}
"#;

/// A92: the SYNC unleased handle mirror — what its first lease costs, what
/// each of the three answers does to it, and that two leases are one call.
///
/// - **Unleased is free.** `unleased_source` is what a handle stub hands back:
///   a mirror carrying its call and having made none. `minted:` reports no
///   ask, no capability and `Waiting` — a handle nothing watches has not
///   merely opened no channel, it has not asked, and costs nothing on either
///   side. Before A92 the stub made the call at the call site and left a
///   capability standing for the life of the connection.
/// - **The first lease is the mint, through the SHIPPED re-mint path.**
///   `acquire` → `remint` → `rebind` is R3's, unchanged; all A92 did was mint
///   the mirror already `released` so that path runs the first time too. That
///   is why the shape costs nothing to add.
/// - **`Failed` and `Absent`, and the retry.** A sync stub has no `Result` to
///   hand a failure back in, so what the call was told is `status()`:
///   `Status::Failed(RpcError::Remote("no route"))` for a call that failed, `Absent` for a
///   `None` reply, with `get()` still empty and no capability minted. Both are
///   answers about NOW — the next 0→1 asks again (`asks` 1 → 2 → 3), and the
///   third one lands: a channel, a seed, `Ready`.
/// - **The join.** Two leases taken while the mint is in flight are ONE call
///   (`in-flight:asks=4` and `joined:asks=4`): `remint` clears `released`
///   before it awaits, so the second `acquire` finds nothing to re-issue, and
///   the rebind subscribes for the demand that exists when it lands.
#[test]
fn an_unleased_mirrors_first_lease_is_its_mint_and_a_failed_one_retries() {
    let stdout = run_program("unleasedmint", UNLEASED_MINT);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "minted:asks=0 sources=0 status=Status::Waiting",
            "failed:asks=1 sources=0 status=Status::Failed(RpcError::Remote(\"no route\"))",
            "absent:asks=2 sources=0 status=Status::Absent held=false",
            "value:first",
            "ready:asks=3 sources=1 status=Status::Ready held=first",
            "closed:sources=0",
            // The ask is synchronous in this seam, so `asks` has already moved
            // when the second lease is taken; the two observers are seeded by
            // the rebind that lands after it.
            "in-flight:asks=4",
            "left:first",
            "right:first",
            "joined:asks=4 sources=1 live=1",
            "parted:sources=0 live=0",
            "done",
        ],
        "the unleased mint went differently:\n{stdout}"
    );
}

// --- A114: a counted mirror lease is the RUN's, not the boundary's ----------

/// `scoped_effect` over the one subscription in the system that costs a network
/// frame (tracker A114): a `RemoteSource` lease. The link counts `Subscribe` and
/// `Unsubscribe` frames going up, so "released at the run's end" is a fact about
/// the wire rather than about a count in the client.
///
/// The control is a plain `effect` in the same program shape, run by hand: it
/// reads `up=1 down=0` at every step, because all three leases are the
/// BOUNDARY's and the channel is opened once and closed once — and after three
/// runs one update reaches THREE observers (`run 0 sees 5`, `run 1 sees 5`,
/// `run 2 sees 5`) instead of one. That is the accumulation `scoped_effect`
/// exists to stop, and it is what makes `run 2 sees 5` on its own the load-
/// bearing line here.
const A114_SCOPED_LEASE: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Disposable, Owner, Signal, SignalCell, Source, run_with_owner };
import std::rpc::{ DuplexEnd, ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };
import std::shared::Shared;
import std::wire::Frame;

fun frame_text(frame: Frame): str {
	match frame {
		Frame::Text(let value) => value,
		Frame::Binary(let _bytes) => "",
	}
}

fun counting_link(subscribes: Shared<i32>, unsubscribes: Shared<i32>): (DuplexEnd, DuplexEnd) {
	let (client_end, client_relay) = duplex_pair();
	let (server_end, server_relay) = duplex_pair();
	client_relay.on_frame(|frame| {
		let text = frame_text(frame);
		if text.contains("Unsubscribe") {
			unsubscribes.write() = unsubscribes.read() + 1;
		} else if text.contains("Subscribe") {
			subscribes.write() = subscribes.read() + 1;
		}
		server_relay.send(frame);
	});
	server_relay.on_frame(|frame| client_relay.send(frame));
	(client_end, server_end)
}

fun main() {
	let subscribes: Shared<i32> = Shared::new(0);
	let unsubscribes: Shared<i32> = Shared::new(0);
	let (client_end, server_end) = counting_link(subscribes, unsubscribes);
	let session = ReactiveServer::new(server_end, json_codec());
	let client = ReactiveClient::new(client_end, json_codec());
	let cell: SignalCell<i32> = Signal::new(1);
	let channel = session.expose(cell);
	let mirror: RemoteSource<i32> = client.source(channel);

	let key: SignalCell<i32> = Signal::new(0);
	let boundary = Owner::new();
	run_with_owner(boundary, || {
		key.effect(|run: i32| {
			mirror.effect(|value: Option<i32>| {
				print(i"run {run} sees {value.unwrap_or(0)}");
			});
		});
	});
	print(i"first up={subscribes.read()} down={unsubscribes.read()}");
	key.set(1);
	print(i"second up={subscribes.read()} down={unsubscribes.read()}");
	key.set(2);
	print(i"third up={subscribes.read()} down={unsubscribes.read()}");
	// ONE observer is live after three runs, not three: the previous runs'
	// leases were released with their owners.
	cell.set(5);
	boundary.dispose();
	print(i"disposed up={subscribes.read()} down={unsubscribes.read()}");
	cell.set(9);
	print("done");
}
"#;

#[test]
fn a114_a_mirror_lease_taken_in_a_scoped_effect_is_released_at_the_runs_end() {
    let stdout = run_program("a114_scoped_lease", A114_SCOPED_LEASE);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            // Run 0 takes the lease: one `Subscribe` on the wire.
            "run 0 sees 1",
            "first up=1 down=0",
            // Run 1 replaces it: run 0's lease is released as its owner goes,
            // and the new run takes its own.
            "run 1 sees 1",
            "second up=2 down=1",
            "run 2 sees 1",
            "third up=3 down=2",
            // ONE observer is live after three runs. A plain `effect` prints
            // three lines here.
            "run 2 sees 5",
            // The boundary releases the last run's lease: three up, three down.
            "disposed up=3 down=3",
            "done",
        ],
        "a mirror lease taken inside a scoped effect must be released when that \
         run ends, and the last one by the boundary; got:\n{stdout}"
    );
}

// --- A134: a handle stub is idempotent per ORIGIN ---------------------------

/// A134, IN PROCESS (a `duplex_pair` link and a `local_rpc` transport stamped
/// for the session — the generated client, reached as a hand-wired in-process
/// client reaches it). Three claims, each its own block: one origin is one
/// mirror; a stub inside a COLD select reads the mirror its lease keeps live;
/// the entry goes with the mirror's last lease.
const A134_ORIGIN_DEDUP: &str = r#"import std::io::print;
import std::json::json_codec;
import std::hash_map::HashMap;
import std::reactive::{ Owner, Signal, SignalCell, Source, owner_scope };
import std::rpc::{ DuplexEnd, LocalTransport, ReactiveClient, RemoteSource, duplex_pair, local_rpc, register_session };
import std::shared::Shared;
import std::time::{ Duration, sleep_for };
import std::wire::Frame;

[service(StoreClient)]
struct Store {
	channels: SignalCell<List<i32>>,
	names: Shared<HashMap<i32, SignalCell<str>>>,
	calls: Shared<i32>,
}

impl Store {
	[rpc]
	fun create_channel(self, id: i32, name: str): i32 {
		self.channels.update(|&mut ids| {
			ids.push(id);
		});
		let cell: SignalCell<str> = Signal::new(name);
		self.names.write().insert(id, cell);
		id
	}

	[rpc]
	fun get_channels(self): SignalCell<List<i32>> {
		self.calls.write() += 1;
		self.channels
	}

	[rpc]
	fun get_name(self, id: i32): Option<SignalCell<str>> {
		self.calls.write() += 1;
		self.names.read().get(id)
	}
}

fun text_of(frame: Frame): str {
	match frame {
		Frame::Text(let text) => text,
		Frame::Binary(let _bytes) => "<binary>",
	}
}

/// The link counts `Subscribe` frames going up — "one mirror" is a claim about
/// the wire, not only about an object.
fun counted_pair(subscribes: Shared<i32>): (DuplexEnd, DuplexEnd) {
	let (client_end, client_relay) = duplex_pair();
	let (server_end, server_relay) = duplex_pair();
	client_relay.on_frame(|frame| {
		let text = text_of(frame);
		if text.contains("Subscribe") && !text.contains("Unsubscribe") {
			subscribes.write() += 1;
		}
		server_relay.send(frame);
	});
	server_relay.on_frame(|frame| client_relay.send(frame));
	(client_end, server_end)
}

fun same<T>(left: RemoteSource<T>, right: RemoteSource<T>): bool {
	left.count.identity() == right.count.identity()
}

fun main() {
	let store = Store { channels = Signal::new([]), names = Shared::new(HashMap::new()), calls = Shared::new(0) };
	let subscribes: Shared<i32> = Shared::new(0);
	let (client_end, server_end) = counted_pair(subscribes);
	register_session(7, server_end, json_codec());
	let transport = local_rpc(store.dispatcher().into_protocol(json_codec()).for_connection(7));
	let reactive = ReactiveClient::new(client_end, json_codec());
	let client = StoreClient { transport, codec = json_codec(), reactive };

	// (1) Identity: one origin is one mirror; another argument is another origin.
	print(i"same-origin:{same(client.get_channels(), client.get_channels())}");
	print(i"same-args:{same(client.get_name(1), client.get_name(1))}");
	print(i"other-args:{same(client.get_name(1), client.get_name(2))}");
	print(i"unleased:calls={store.calls.read()} subscribes={subscribes.read()}");

	// (2) The kolt shape: a stub inside a COLD select, pulled on every read.
	let client_cell: SignalCell<Option<StoreClient<LocalTransport>>> = Signal::new(Some(client));
	let page = Owner::new();
	owner_scope.run(page, || {
		let ids: SignalCell<List<i32>> = client_cell
			.and_then(|held| held.get_channels())
			.derive(|value| value.unwrap_or_default())
			.cell();
		ids.effect(|value| print(i"cold-select sees {value.len()}"));
	});
	sleep_for(Duration::millis(0));
	print(i"create:{client.create_channel(1, "general").unwrap_or(0)}");
	print(i"create:{client.create_channel(2, "random").unwrap_or(0)}");
	print(i"leased:calls={store.calls.read()} subscribes={subscribes.read()}");

	// (3) The entry goes with the mirror's last lease: after the page is gone
	// and the close has flushed, the next stub call mints afresh.
	let before = client.get_channels();
	page.dispose();
	sleep_for(Duration::millis(0));
	sleep_for(Duration::millis(0));
	print(i"released:fresh={!same(before, client.get_channels())}");
	print("done");
}
"#;

/// A134 (door a): the generated client dedups the mirrors it mints per ORIGIN —
/// the method and its described arguments — so a stub call is idempotent.
///
/// - **One origin, one mirror.** Two `get_channels()` calls are one object (the
///   count cell's identity), two `get_name(1)`s are one, and `get_name(1)` /
///   `get_name(2)` are two: the arguments are part of the origin. Unleased,
///   nothing was asked (`calls=0`) — dedup does not make a stub eager.
/// - **The cold select.** `and_then` runs its select on every PULL, so before
///   the table every pull minted a fresh mirror: `.cell()` read one, its
///   `on_settle` leased a second, and each refresh read a third, so the cell
///   read `0` forever while the leased mirror went unread (the red read
///   `cold-select sees 0` after both creates). With the table every pull
///   answers the one mirror its lease keeps live — `0`, `1`, `2` — on ONE
///   call and ONE `Subscribe`.
/// - **Released with the last lease.** After the page owner is disposed and
///   the close has flushed (the settle, then the microtask hop), the next stub
///   call mints a fresh mirror: the table holds what the app watches, not
///   everything it ever watched.
///
/// The socket twin is the next test.
#[test]
fn a134_two_stub_calls_for_one_origin_are_one_mirror_and_one_subscribe() {
    let stdout = run_program("a134_origin", A134_ORIGIN_DEDUP);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "same-origin:true",
            "same-args:true",
            "other-args:false",
            "unleased:calls=0 subscribes=0",
            // The effect's immediate call (nothing has arrived, so the
            // default), then the seed.
            "cold-select sees 0",
            "cold-select sees 0",
            "cold-select sees 1",
            "create:1",
            "cold-select sees 2",
            "create:2",
            "leased:calls=1 subscribes=1",
            "released:fresh=true",
            "done",
        ],
        "a stub call must be idempotent per origin; got:\n{stdout}"
    );
}

// --- B519: the service in a module the entry IMPORTS -------------------------

/// The A134 store, as its own module: the shape every multi-file app writes and
/// no A134 pin did — they were all single-file, which is why none saw a table
/// the generated code read and the bundle never declared.
const B519_STORE_MODULE: &str = include_str!("native/b519_store.vl");

const B519_IMPORTING_MAIN: &str = include_str!("native/b519_imported_service.vl");

/// B519, RUN: A134's identity claims with the service declared in a module
/// the entry imports. The table each stub reads (`__mirrors_StoreClient_*`)
/// was never declared there, so the first stub call threw `ReferenceError` —
/// the owner's `__mirrors_KoltClient_get_channels is not defined`. Two calls
/// for one origin are one mirror, another argument another, and minting
/// asked nothing of the server.
#[test]
fn b519_a_service_in_an_imported_module_mints_one_mirror_per_origin() {
    let stdout = run_package(
        "b519_imported_service",
        &[
            ("src/main.vl", B519_IMPORTING_MAIN),
            ("src/b519_store.vl", B519_STORE_MODULE),
        ],
    );
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "same-origin:true",
            "same-args:true",
            "other-args:false",
            "unleased:calls=0",
            "done",
        ],
        "a stub of a service declared in an imported module must be idempotent \
         per origin; got:\n{stdout}"
    );
}

/// B519, the FULL-STACK shape: `vilan build .` on a two-entry package whose
/// service lives in a shared module. Each leg's output declares every mirror
/// table it reads (the generic check every example and golden is also held
/// to), and the client leg — the one whose stubs read them — reads at least
/// one, so the claim is not vacuous.
#[test]
fn b519_both_legs_of_a_fullstack_build_declare_every_mirror_table_they_read() {
    let dir = temp_project("b519_fullstack");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"mir\"\ndefault-entry = \"client\"\n\n\
         [entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/b519_store.vl", B519_STORE_MODULE);
    write(
        &dir,
        "src/client.vl",
        r#"import std::io::print;
import std::json::json_codec;
import std::reactive::Source;
import pkg::b519_store::StoreClient;

fun main() {
	match StoreClient::connect("/", json_codec()) {
		Ok(let client) => {
			print(client.get_channels().get().is_some());
			print(client.get_name(1).get().is_some());
		}
		Err(let failure) => print("no"),
	}
}
"#,
    );
    write(
        &dir,
        "src/server.vl",
        r#"import std::io::print;
import std::hash_map::HashMap;
import std::reactive::Signal;
import std::shared::Shared;
import pkg::b519_store::Store;

fun main() {
	let store = Store { channels = Signal::new([]), names = Shared::new(HashMap::new()), calls = Shared::new(0) };
	print(store.calls.read());
}
"#,
    );
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", dir.to_str().unwrap()])
        .output()
        .expect("run vilan build");
    assert!(
        output.status.success(),
        "the build failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let client = std::fs::read_to_string(dir.join("dist/client.js")).expect("the client leg");
    assert!(
        dir.join("dist/server.mjs").is_file(),
        "the server leg was not emitted"
    );
    let (read, _) = support::mirror_tables::mirror_tables(&client);
    let dangling = support::mirror_tables::dangling_in_tree(&dir.join("dist"));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        read.contains("__mirrors_StoreClient_get_channels")
            && read.contains("__mirrors_StoreClient_get_name"),
        "the client leg's stubs must read their tables (else this pin is vacuous): {read:?}"
    );
    assert!(
        dangling.is_empty(),
        "emitted legs read mirror tables they never declare:\n{}",
        dangling.join("\n")
    );
}

/// A134 OVER A SOCKET: the same cold-select shape, server and client in one
/// process joined by a real WebSocket (`Server` on port 0, the generated
/// `connect`). The in-process pin above is not evidence for this one (the
/// order's rule: an in-process probe is not a socket probe), and this is the
/// transport kolt runs on.
const A134_ORIGIN_DEDUP_SOCKET: &str = r#"import std::io::print;
import std::json::json_codec;
import std::http::{ Response, Server };
import std::process::exit;
import std::reactive::{ Owner, Signal, SignalCell, Source, owner_scope };
import std::result::Result::{ self, Ok, Err };
import std::rpc::{ RemoteSource, SocketTransport };
import std::rpc::server::Service;
import std::shared::Shared;
import std::time::sleep;

[service(StoreClient)]
struct Store {
	channels: SignalCell<List<i32>>,
	calls: Shared<i32>,
}

impl Store {
	[rpc]
	fun create_channel(self, id: i32): i32 {
		self.channels.update(|&mut ids| {
			ids.push(id);
		});
		id
	}

	[rpc]
	fun get_channels(self): SignalCell<List<i32>> {
		self.calls.write() += 1;
		self.channels
	}

	[rpc]
	fun calls(self): i32 {
		self.calls.read()
	}
}

let store: Store = Store { channels = Signal::new([]), calls = Shared::new(0) };

fun main() {
	Server::builder()
		.port(0)
		.with_service(Service::new(store.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

fun same<T>(left: RemoteSource<T>, right: RemoteSource<T>): bool {
	left.count.identity() == right.count.identity()
}

async fun run(port: i32) {
	match StoreClient::connect(i"ws://localhost:{port}/", json_codec()) {
		Ok(let client) => {
			print(i"same-origin:{same(client.get_channels(), client.get_channels())}");
			let client_cell: SignalCell<Option<StoreClient<SocketTransport>>> = Signal::new(Some(client));
			let page = Owner::new();
			let seen: Shared<i32> = Shared::new(0);
			owner_scope.run(page, || {
				let ids: SignalCell<List<i32>> = client_cell
					.and_then(|held| held.get_channels())
					.derive(|value| value.unwrap_or_default())
					.cell();
				ids.effect(|value| {
					seen.write() += 1;
					print(i"cold-select sees {value.len()}");
				});
			});
			// The seed is the mint's round trip plus the `Subscribe`'s; wait
			// for it by what it does, never by a duration. After it, each
			// create's `Update` precedes its reply on the one socket.
			mut tries = 0;
			for seen.read() < 2 && tries < 1000 {
				sleep(10);
				tries += 1;
			}
			print(i"create:{client.create_channel(1).unwrap_or(0)}");
			print(i"create:{client.create_channel(2).unwrap_or(0)}");
			print(i"calls:{client.calls().unwrap_or(0 - 1)}");
			page.dispose();
		},
		Err(let error) => print(i"err:{error.debug()}"),
	}
	exit(0);
}
"#;

#[test]
fn a134_a_stub_in_a_cold_select_reads_its_leased_mirror_over_a_socket() {
    let stdout = run_program("a134_socket", A134_ORIGIN_DEDUP_SOCKET);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "same-origin:true",
            "cold-select sees 0",
            "cold-select sees 0",
            "cold-select sees 1",
            "create:1",
            "cold-select sees 2",
            "create:2",
            "calls:1",
        ],
        "over a socket, a stub inside a cold select must read the mirror its \
         lease keeps live, on one call; got:\n{stdout}"
    );
}

// --- A137: a mirror that JOINS a held forward is seeded ---------------------

/// A137 IN PROCESS: two ORIGINS (`notes` / `notes_again`, `rows` /
/// `rows_again`) answering ONE cell, so `expose_dynamic` puts both mirrors on
/// one channel while the client (A134) keeps two. The socket twin is next.
const A137_JOIN_SEED: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell };
import std::result::Result::{ self, Ok, Err };
import std::rpc::{ KeyedCell, KeyedSource, ReactiveClient, RemoteSource, duplex_pair, local_rpc, register_session };
import std::shared::Shared;
import std::time::{ Duration, sleep_for };
import std::wire::{ Keyed, Wire };

[derive(Wire, PartialEq)]
struct Row {
	id: str,
	text: str,
}

impl Row with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

// Two methods answering ONE cell: `expose_dynamic` dedups them onto one
// channel, and they are two ORIGINS, so the client keeps two mirrors.
[service(BoardClient)]
struct Board {
	notes: SignalCell<List<str>>,
	rows: KeyedCell<str, Row>,
}

impl Board {
	[rpc]
	fun add(self, text: str): i32 {
		self.notes.update(|&mut list| {
			list.push(text);
		});
		self.rows.insert(Row { id = text, text });
		0
	}

	[rpc]
	fun notes(self): SignalCell<List<str>> {
		self.notes
	}

	[rpc]
	fun notes_again(self): SignalCell<List<str>> {
		self.notes
	}

	[rpc]
	fun rows(self): KeyedCell<str, Row> {
		self.rows
	}

	[rpc]
	fun rows_again(self): KeyedCell<str, Row> {
		self.rows
	}
}

fun size(list: Option<List<str>>): i32 {
	match list {
		Some(let held) => held.len().as_i32(),
		None => 0 - 1,
	}
}

fun rows_size(list: Option<List<Row>>): i32 {
	match list {
		Some(let held) => held.len().as_i32(),
		None => 0 - 1,
	}
}

fun main() {
	let board = Board { notes = Signal::new(["seed"]), rows = KeyedCell::new([Row { id = "seed", text = "seed" }]) };
	let (client_end, server_end) = duplex_pair();
	register_session(3, server_end, json_codec());
	let transport = local_rpc(board.dispatcher().into_protocol(json_codec()).for_connection(3));
	let client = BoardClient { transport, codec = json_codec(), reactive = ReactiveClient::new(client_end, json_codec()) };

	let first: RemoteSource<List<str>> = client.notes();
	let second: RemoteSource<List<str>> = client.notes_again();
	let _first = first.sub(|list| {});
	sleep_for(Duration::millis(0));
	let _second = second.sub(|list| {});
	sleep_for(Duration::millis(0));
	print(i"plain: same-channel={first.channel.read() == second.channel.read()} first={size(first.get())} second={size(second.get())}");
	let rows: KeyedSource<str, Row> = client.rows();
	let rows_again: KeyedSource<str, Row> = client.rows_again();
	let _rows = rows.sub(|list| {});
	sleep_for(Duration::millis(0));
	let _rows_again = rows_again.sub(|list| {});
	sleep_for(Duration::millis(0));
	print(i"keyed: same-channel={rows.channel.read() == rows_again.channel.read()} first={rows_size(rows.get())} second={rows_size(rows_again.get())}");
	print(i"add:{client.add("x").unwrap_or(0 - 1)}");
	print(i"after: plain={size(first.get())}/{size(second.get())} keyed={rows_size(rows.get())}/{rows_size(rows_again.get())}");
}
"#;

/// A137, settled: the second mirror's `Subscribe` JOINS the forward the first
/// holds (`LiveForward.holds`) and the server sends it no seed — a frame names
/// a channel, not a mirror, so a re-seed would reach the sibling too. The
/// joiner therefore held nothing until the channel's next change
/// (`second=-1`), and a KEYED joiner was worse than silent: the next patch
/// landed on an empty mirror and left it DESYNCED for good (`keyed=2/1` after
/// the add — the seed row never arrives). Both reproduce over a socket (the
/// next test), so this is the mechanism and not A133's in-process caveat. The
/// fix is the client's, where the joiner can be seeded ALONE: the sibling's
/// route re-encodes what it holds (an `Update`, or a `Patch` of one `Reset`)
/// and the joiner's own deliverer lands it (`seed_from_sibling`). Red before on
/// the four `second=`/`keyed=` values.
#[test]
fn a137_a_mirror_joining_a_held_forward_is_seeded_from_its_sibling() {
    let stdout = run_program("a137_join", A137_JOIN_SEED);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "plain: same-channel=true first=1 second=1",
            "keyed: same-channel=true first=1 second=1",
            "add:0",
            "after: plain=2/2 keyed=2/2",
        ],
        "a mirror whose Subscribe joins a sibling's forward must hold the \
         channel's value; got:\n{stdout}"
    );
}

/// A137 OVER A SOCKET — the run the order asked for before anything was built:
/// server and client in one process over a real WebSocket, the same two
/// origins per cell. Readiness is polled (a mirror's channel is bound the
/// moment its rebind has run, and the join seed is synchronous inside it).
const A137_JOIN_SEED_SOCKET: &str = r#"import std::io::print;
import std::json::json_codec;
import std::http::{ Response, Server };
import std::process::exit;
import std::reactive::{ Signal, SignalCell };
import std::result::Result::{ self, Ok, Err };
import std::rpc::{ KeyedCell, KeyedSource, RemoteSource };
import std::rpc::server::Service;
import std::shared::Shared;
import std::time::sleep;
import std::wire::{ Keyed, Wire };

[derive(Wire, PartialEq)]
struct Row {
	id: str,
	text: str,
}

impl Row with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

// Two methods answering ONE cell: `expose_dynamic` dedups them onto one
// channel, and they are two ORIGINS, so the client keeps two mirrors.
[service(BoardClient)]
struct Board {
	notes: SignalCell<List<str>>,
	rows: KeyedCell<str, Row>,
}

impl Board {
	[rpc]
	fun add(self, text: str): i32 {
		self.notes.update(|&mut list| {
			list.push(text);
		});
		self.rows.insert(Row { id = text, text });
		0
	}

	[rpc]
	fun notes(self): SignalCell<List<str>> {
		self.notes
	}

	[rpc]
	fun notes_again(self): SignalCell<List<str>> {
		self.notes
	}

	[rpc]
	fun rows(self): KeyedCell<str, Row> {
		self.rows
	}

	[rpc]
	fun rows_again(self): KeyedCell<str, Row> {
		self.rows
	}
}

let board: Board = Board { notes = Signal::new(["seed"]), rows = KeyedCell::new([Row { id = "seed", text = "seed" }]) };

fun main() {
	Server::builder()
		.port(0)
		.with_service(Service::new(board.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

/// Wait until `ready` holds, by polling — never by a duration alone.
async fun until(ready: || bool) {
	mut tries = 0;
	for !ready() && tries < 1000 {
		sleep(10);
		tries += 1;
	}
}

fun size(list: Option<List<str>>): i32 {
	match list {
		Some(let held) => held.len().as_i32(),
		None => 0 - 1,
	}
}

fun rows_size(list: Option<List<Row>>): i32 {
	match list {
		Some(let held) => held.len().as_i32(),
		None => 0 - 1,
	}
}

async fun run(port: i32) {
	match BoardClient::connect(i"ws://localhost:{port}/", json_codec()) {
		Ok(let client) => {
			// PLAIN. The first mirror leases and is seeded by the server.
			let first: RemoteSource<List<str>> = client.notes();
			let second: RemoteSource<List<str>> = client.notes_again();
			let _first = first.sub(|list| {});
			until(|| first.get().is_some());
			// The JOIN: the second mirror's mint lands on the SAME channel and
			// its `Subscribe` joins the forward the first holds — the server
			// sends no seed for it. `rebind` runs the join synchronously, so the
			// moment the channel is bound is the moment to read.
			let _second = second.sub(|list| {});
			until(|| second.channel.read() >= 0);
			print(i"plain: same-channel={first.channel.read() == second.channel.read()} first={size(first.get())} second={size(second.get())}");
			// KEYED, the same shape through a whole-collection lease.
			let rows: KeyedSource<str, Row> = client.rows();
			let rows_again: KeyedSource<str, Row> = client.rows_again();
			let _rows = rows.sub(|list| {});
			until(|| rows.get().is_some());
			let _rows_again = rows_again.sub(|list| {});
			until(|| rows_again.channel.read() >= 0);
			print(i"keyed: same-channel={rows.channel.read() == rows_again.channel.read()} first={rows_size(rows.get())} second={rows_size(rows_again.get())}");
			// And after a change both carry it, as they did before.
			print(i"add:{client.add("x").unwrap_or(0 - 1)}");
			until(|| size(second.get()) == 2 && rows_size(rows_again.get()) == 2);
			print(i"after: plain={size(first.get())}/{size(second.get())} keyed={rows_size(rows.get())}/{rows_size(rows_again.get())}");
		},
		Err(let error) => print(i"err:{error.debug()}"),
	}
	exit(0);
}
"#;

#[test]
fn a137_a_joining_mirror_is_seeded_over_a_socket() {
    let stdout = run_program("a137_join_socket", A137_JOIN_SEED_SOCKET);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "plain: same-channel=true first=1 second=1",
            "keyed: same-channel=true first=1 second=1",
            "add:0",
            "after: plain=2/2 keyed=2/2",
        ],
        "over a socket, a mirror whose Subscribe joins a sibling's forward must \
         hold the channel's value; got:\n{stdout}"
    );
}

// --- A139: a PER-KEY lease joining a sibling's per-key forward is seeded -----

/// A139 IN PROCESS: A137's two origins on one keyed cell, now per KEY. The
/// first mirror leases key `a` (its per-key `Subscribe` starts the forward and
/// the server seeds it); the second then leases the same key, and its
/// `Subscribe` JOINS that forward (`LiveForward.holds` is per demand), so the
/// server sends it nothing. Before the fix the joiner held nothing under the
/// key (`second=-`, `Waiting`) and the key's next `Update` landed on a mirror
/// that did not hold it: a protocol FAULT, desynced for good. The socket twin
/// is next.
const A139_KEY_JOIN_SEED: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell };
import std::result::Result::{ self, Ok, Err };
import std::rpc::{ KeyedCell, KeyedSource, ReactiveClient, duplex_pair, local_rpc, register_session };
import std::shared::Shared;
import std::time::{ Duration, sleep_for };
import std::wire::{ Keyed, Wire };

[derive(Wire, PartialEq)]
struct Row {
	id: str,
	text: str,
}

impl Row with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

[service(BoardClient)]
struct Board {
	rows: KeyedCell<str, Row>,
}

impl Board {
	[rpc]
	fun edit(self, id: str, text: str): i32 {
		self.rows.update(id, |&mut row| {
			row.text = text;
		});
		0
	}

	[rpc]
	fun rows(self): KeyedCell<str, Row> {
		self.rows
	}

	[rpc]
	fun rows_again(self): KeyedCell<str, Row> {
		self.rows
	}
}

fun text_of(row: Option<Row>): str {
	match row {
		Some(let held) => held.text,
		None => "-",
	}
}

fun fault_of(fault: Option<str>): str {
	match fault {
		Some(let reason) => reason,
		None => "none",
	}
}

fun main() {
	let board = Board { rows = KeyedCell::new([Row { id = "a", text = "one" }, Row { id = "b", text = "two" }]) };
	let (client_end, server_end) = duplex_pair();
	register_session(3, server_end, json_codec());
	let transport = local_rpc(board.dispatcher().into_protocol(json_codec()).for_connection(3));
	let client = BoardClient { transport, codec = json_codec(), reactive = ReactiveClient::new(client_end, json_codec()) };
	let first: KeyedSource<str, Row> = client.rows();
	let second: KeyedSource<str, Row> = client.rows_again();
	let seen_first: Shared<str> = Shared::new("?");
	let seen_second: Shared<str> = Shared::new("?");
	let _first = first.sub_key("a", |row| seen_first.write() = text_of(row));
	sleep_for(Duration::millis(0));
	let _second = second.sub_key("a", |row| seen_second.write() = text_of(row));
	sleep_for(Duration::millis(0));
	print(i"join: same-channel={first.channel.read() == second.channel.read()} first={seen_first.read()} second={seen_second.read()} second-status={second.known().debug()}");
	print(i"edit:{client.edit("a", "uno").unwrap_or(0 - 1)}");
	sleep_for(Duration::millis(0));
	print(i"after: first={seen_first.read()} second={seen_second.read()} fault={fault_of(second.fault())}");
}
"#;

#[test]
fn a139_a_per_key_lease_joining_a_siblings_forward_is_seeded() {
    let stdout = run_program("a139_key_join", A139_KEY_JOIN_SEED);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "join: same-channel=true first=one second=one second-status=Status::Ready",
            "edit:0",
            "after: first=uno second=uno fault=none",
        ],
        "a per-key lease whose Subscribe joins a sibling's per-key forward must \
         hold the key's element; got:\n{stdout}"
    );
}

/// A139 OVER A SOCKET: the same two per-key leases over a real WebSocket.
/// Red before on the same three values (`second=-`, `Waiting`, the fault).
const A139_KEY_JOIN_SEED_SOCKET: &str = r#"import std::io::print;
import std::json::json_codec;
import std::http::{ Response, Server };
import std::process::exit;
import std::result::Result::{ self, Ok, Err };
import std::rpc::{ KeyedCell, KeyedSource };
import std::rpc::server::Service;
import std::shared::Shared;
import std::time::sleep;
import std::wire::{ Keyed, Wire };

[derive(Wire, PartialEq)]
struct Row {
	id: str,
	text: str,
}

impl Row with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

[service(BoardClient)]
struct Board {
	rows: KeyedCell<str, Row>,
}

impl Board {
	[rpc]
	fun edit(self, id: str, text: str): i32 {
		self.rows.update(id, |&mut row| {
			row.text = text;
		});
		0
	}

	[rpc]
	fun rows(self): KeyedCell<str, Row> {
		self.rows
	}

	[rpc]
	fun rows_again(self): KeyedCell<str, Row> {
		self.rows
	}
}

let board: Board = Board { rows = KeyedCell::new([Row { id = "a", text = "one" }, Row { id = "b", text = "two" }]) };

fun main() {
	Server::builder()
		.port(0)
		.with_service(Service::new(board.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

async fun until(ready: || bool) {
	mut tries = 0;
	for !ready() && tries < 100 {
		sleep(10);
		tries += 1;
	}
}

fun text_of(row: Option<Row>): str {
	match row {
		Some(let held) => held.text,
		None => "-",
	}
}

fun fault_of(fault: Option<str>): str {
	match fault {
		Some(let reason) => reason,
		None => "none",
	}
}

async fun run(port: i32) {
	match BoardClient::connect(i"ws://localhost:{port}/", json_codec()) {
		Ok(let client) => {
			let first: KeyedSource<str, Row> = client.rows();
			let second: KeyedSource<str, Row> = client.rows_again();
			let seen_first: Shared<str> = Shared::new("?");
			let seen_second: Shared<str> = Shared::new("?");
			let _first = first.sub_key("a", |row| seen_first.write() = text_of(row));
			until(|| seen_first.read() == "one");
			let _second = second.sub_key("a", |row| seen_second.write() = text_of(row));
			until(|| second.channel.read() >= 0);
			until(|| seen_second.read() == "one");
			print(i"join: same-channel={first.channel.read() == second.channel.read()} first={seen_first.read()} second={seen_second.read()} second-status={second.known().debug()}");
			print(i"edit:{client.edit("a", "uno").unwrap_or(0 - 1)}");
			until(|| seen_first.read() == "uno" && (seen_second.read() == "uno" || second.fault().is_some()));
			print(i"after: first={seen_first.read()} second={seen_second.read()} fault={fault_of(second.fault())}");
		},
		Err(let error) => print(i"err:{error.debug()}"),
	}
	exit(0);
}
"#;

#[test]
fn a139_a_per_key_joiner_is_seeded_over_a_socket() {
    let stdout = run_program("a139_key_join_socket", A139_KEY_JOIN_SEED_SOCKET);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "join: same-channel=true first=one second=one second-status=Status::Ready",
            "edit:0",
            "after: first=uno second=uno fault=none",
        ],
        "over a socket, a per-key lease whose Subscribe joins a sibling's \
         per-key forward must hold the key's element; got:\n{stdout}"
    );
}

// --- A143: ONE forward per channel, ops fanned out by each mirror's demand ---

/// A143 IN PROCESS (ruled door b): two mirrors on one keyed channel with
/// DIFFERENT demands — the first holds the whole collection, the second one
/// key. Each used to put its own forward on the server (`forwards=2`), a frame
/// names a channel and not a forward, so every op on `a` reached both mirrors
/// twice: the second `Remove` faulted on the first mirror, and the second
/// mirror took `b`'s `Update` too and faulted on it (and was notified for it).
/// Now the client keeps one forward per channel at the union of its mirrors'
/// demands (`WireDemand`; whole subsumes per-key), and each mirror takes only
/// its own demand's ops (`KeyedSource::accept`): the whole-collection mirror
/// is notified once per op (seed, three edits = 4), the per-key one once per
/// op on its key (the immediate call, the rebind's reset, the seed, the edit,
/// the drop = 5; `b`'s edit reaches it not at all). When the whole lease
/// goes, the wire hands `a` back to a per-key forward. Red before on
/// `forwards=2`, both faults and `notified: first=5 second=6`.
const A143_ONE_FORWARD: &str = r#"import std::io::print;
import std::hash::Hashable;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell };
import std::result::Result::{ self, Ok, Err };
import std::rpc::{ KeyedCell, KeyedSource, ReactiveClient, duplex_pair, local_rpc, register_session, session_of };
import std::shared::Shared;
import std::time::{ Duration, sleep_for };
import std::wire::{ Keyed, Wire };

[derive(Wire, PartialEq)]
struct Row {
	id: str,
	text: str,
}

impl Row with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

// Two methods answering ONE cell: one channel, two origins, two mirrors.
[service(BoardClient)]
struct Board {
	rows: KeyedCell<str, Row>,
}

impl Board {
	[rpc]
	fun edit(self, id: str, text: str): i32 {
		self.rows.update(id, |&mut row| {
			row.text = text;
		});
		0
	}

	[rpc]
	fun drop(self, id: str): i32 {
		self.rows.remove(id);
		0
	}

	[rpc]
	fun rows(self): KeyedCell<str, Row> {
		self.rows
	}

	[rpc]
	fun rows_again(self): KeyedCell<str, Row> {
		self.rows
	}
}

fun text_of(row: Option<Row>): str {
	match row {
		Some(let held) => held.text,
		None => "-",
	}
}

fun fault_of(fault: Option<str>): str {
	match fault {
		Some(let reason) => reason,
		None => "none",
	}
}

fun held(list: Option<List<Row>>): usize {
	match list {
		Some(let rows) => rows.len(),
		None => 0,
	}
}

fun forwards(): usize {
	match session_of(3) {
		Some(let session) => session.live.read().len(),
		None => 0,
	}
}

fun settle() {
	sleep_for(Duration::millis(0));
	sleep_for(Duration::millis(0));
}

fun main() {
	let board = Board { rows = KeyedCell::new([Row { id = "a", text = "one" }, Row { id = "b", text = "two" }]) };
	let (client_end, server_end) = duplex_pair();
	register_session(3, server_end, json_codec());
	let transport = local_rpc(board.dispatcher().into_protocol(json_codec()).for_connection(3));
	let client = BoardClient { transport, codec = json_codec(), reactive = ReactiveClient::new(client_end, json_codec()) };
	let first: KeyedSource<str, Row> = client.rows();
	let second: KeyedSource<str, Row> = client.rows_again();
	let whole_seen: Shared<i32> = Shared::new(0);
	let key_seen: Shared<List<str>> = Shared::new([]);
	// The WHOLE collection on one mirror, one KEY on its sibling.
	let _whole = first.sub(|list| whole_seen.write() += 1);
	settle();
	let _key = second.sub_key("a", |row| key_seen.write().push(text_of(row)));
	settle();
	print(i"mixed: same-channel={first.channel.read() == second.channel.read()} forwards={forwards()} first={held(first.get())} second={held(second.get())} a={text_of(second.pick(second.get(), "a".hash()))}");
	print(i"edit a:{client.edit("a", "uno").unwrap_or(0 - 1)}");
	settle();
	print(i"edit b:{client.edit("b", "dos").unwrap_or(0 - 1)}");
	settle();
	print(i"drop a:{client.drop("a").unwrap_or(0 - 1)}");
	settle();
	print(i"after: first={held(first.get())} second={held(second.get())} first-fault={fault_of(first.fault())} second-fault={fault_of(second.fault())}");
	print(i"notified: first={whole_seen.read()} second={key_seen.read().len()}");
	for seen in key_seen.read() {
		print(i"  second saw {seen}");
	}
	// The whole lease goes: the wire hands the key back to a per-key forward.
	_whole.dispose();
	settle();
	settle();
	print(i"key only: forwards={forwards()} second={held(second.get())} first-fault={fault_of(first.fault())} second-fault={fault_of(second.fault())}");
}
"#;

#[test]
fn a143_mixed_demands_on_one_keyed_channel_share_one_forward_and_fan_out_by_demand() {
    let stdout = run_program("a143_one_forward", A143_ONE_FORWARD);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "mixed: same-channel=true forwards=1 first=2 second=1 a=one",
            "edit a:0",
            "edit b:0",
            "drop a:0",
            "after: first=1 second=0 first-fault=none second-fault=none",
            "notified: first=4 second=5",
            "second saw -",
            "second saw -",
            "second saw one",
            "second saw uno",
            "second saw -",
            "key only: forwards=1 second=0 first-fault=none second-fault=none",
        ],
        "two demands on one keyed channel must share one forward, and each \
         mirror must receive each of its ops exactly once; got:\n{stdout}"
    );
}

/// A143 OVER A SOCKET: the same two demands over a real WebSocket. Red before
/// on both faults and `notified: first=7 second=8`.
const A143_ONE_FORWARD_SOCKET: &str = r#"import std::io::print;
import std::debug::Debug;
import std::hash::Hashable;
import std::json::json_codec;
import std::http::{ Response, Server };
import std::process::exit;
import std::result::Result::{ self, Ok, Err };
import std::rpc::{ KeyedCell, KeyedSource };
import std::rpc::server::Service;
import std::shared::Shared;
import std::time::sleep;
import std::wire::{ Keyed, Wire };

[derive(Wire, PartialEq)]
struct Row {
	id: str,
	text: str,
}

impl Row with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

[service(BoardClient)]
struct Board {
	rows: KeyedCell<str, Row>,
}

impl Board {
	[rpc]
	fun edit(self, id: str, text: str): i32 {
		self.rows.update(id, |&mut row| {
			row.text = text;
		});
		0
	}

	[rpc]
	fun drop(self, id: str): i32 {
		self.rows.remove(id);
		0
	}

	[rpc]
	fun rows(self): KeyedCell<str, Row> {
		self.rows
	}

	[rpc]
	fun rows_again(self): KeyedCell<str, Row> {
		self.rows
	}
}

let board: Board = Board { rows = KeyedCell::new([Row { id = "a", text = "one" }, Row { id = "b", text = "two" }]) };

fun main() {
	Server::builder()
		.port(0)
		.with_service(Service::new(board.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

async fun until(ready: || bool) {
	mut tries = 0;
	for !ready() && tries < 100 {
		sleep(10);
		tries += 1;
	}
}

fun text_of(row: Option<Row>): str {
	match row {
		Some(let held) => held.text,
		None => "-",
	}
}

fun held(list: Option<List<Row>>): usize {
	match list {
		Some(let rows) => rows.len(),
		None => 0,
	}
}

fun fault_of(fault: Option<str>): str {
	match fault {
		Some(let reason) => reason,
		None => "none",
	}
}

async fun run(port: i32) {
	match BoardClient::connect(i"ws://localhost:{port}/", json_codec()) {
		Ok(let client) => {
			let first: KeyedSource<str, Row> = client.rows();
			let second: KeyedSource<str, Row> = client.rows_again();
			let whole_seen: Shared<i32> = Shared::new(0);
			let key_seen: Shared<List<str>> = Shared::new([]);
			let _whole = first.sub(|list| whole_seen.write() += 1);
			until(|| first.get().is_some());
			let _key = second.sub_key("a", |row| key_seen.write().push(text_of(row)));
			until(|| second.channel.read() >= 0 && held(second.get()) == 1);
			print(i"mixed: same-channel={first.channel.read() == second.channel.read()} first={held(first.get())} second={held(second.get())} a={text_of(second.pick(second.get(), "a".hash()))}");
			print(i"edit a:{client.edit("a", "uno").unwrap_or(0 - 1)}");
			until(|| text_of(second.pick(second.get(), "a".hash())) == "uno");
			print(i"edit b:{client.edit("b", "dos").unwrap_or(0 - 1)}");
			until(|| whole_seen.read() >= 3);
			print(i"drop a:{client.drop("a").unwrap_or(0 - 1)}");
			until(|| held(first.get()) == 1 && held(second.get()) == 0);
			sleep(50);
			print(i"after: first={held(first.get())} second={held(second.get())} first-fault={fault_of(first.fault())} second-fault={fault_of(second.fault())}");
			print(i"notified: first={whole_seen.read()} second={key_seen.read().len()}");
			for seen in key_seen.read() {
				print(i"  second saw {seen}");
			}
		},
		Err(let error) => print(i"err:{error.debug()}"),
	}
	exit(0);
}
"#;

#[test]
fn a143_mixed_demands_share_one_forward_over_a_socket() {
    let stdout = run_program("a143_one_forward_socket", A143_ONE_FORWARD_SOCKET);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "mixed: same-channel=true first=2 second=1 a=one",
            "edit a:0",
            "edit b:0",
            "drop a:0",
            "after: first=1 second=0 first-fault=none second-fault=none",
            "notified: first=4 second=5",
            "second saw -",
            "second saw -",
            "second saw one",
            "second saw uno",
            "second saw -",
        ],
        "over a socket, two demands on one keyed channel must share one \
         forward and fan out by demand; got:\n{stdout}"
    );
}

/// A143's ordering, on ONE minted (dynamic) keyed mirror holding the whole
/// collection and key `a`: releasing the whole lease used to send the whole
/// `Unsubscribe` FIRST, which left the channel carrying no forward, so the
/// server REVOKED it, and the key's `Subscribe` that followed named nothing
/// (`forwards=0`, `a=-` forever, the edit never arriving). The wire now sends
/// subscribes before unsubscribes. In process; the revocation rule it hits is
/// the server's and transport-independent.
const A143_HANDOFF: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell };
import std::result::Result::{ self, Ok, Err };
import std::rpc::{ KeyedCell, KeyedSource, ReactiveClient, duplex_pair, local_rpc, register_session, session_of };
import std::shared::Shared;
import std::time::{ Duration, sleep_for };
import std::wire::{ Keyed, Wire };

[derive(Wire, PartialEq)]
struct Row {
	id: str,
	text: str,
}

impl Row with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

// Two methods answering ONE cell: one channel, two origins, two mirrors.
[service(BoardClient)]
struct Board {
	rows: KeyedCell<str, Row>,
}

impl Board {
	[rpc]
	fun edit(self, id: str, text: str): i32 {
		self.rows.update(id, |&mut row| {
			row.text = text;
		});
		0
	}

	[rpc]
	fun drop(self, id: str): i32 {
		self.rows.remove(id);
		0
	}

	[rpc]
	fun rows(self): KeyedCell<str, Row> {
		self.rows
	}

	[rpc]
	fun rows_again(self): KeyedCell<str, Row> {
		self.rows
	}
}

fun text_of(row: Option<Row>): str {
	match row {
		Some(let held) => held.text,
		None => "-",
	}
}

fun fault_of(fault: Option<str>): str {
	match fault {
		Some(let reason) => reason,
		None => "none",
	}
}

fun held(list: Option<List<Row>>): usize {
	match list {
		Some(let rows) => rows.len(),
		None => 0,
	}
}

fun forwards(): usize {
	match session_of(3) {
		Some(let session) => session.live.read().len(),
		None => 0,
	}
}

fun settle() {
	sleep_for(Duration::millis(0));
	sleep_for(Duration::millis(0));
}

fun main() {
	let board = Board { rows = KeyedCell::new([Row { id = "a", text = "one" }, Row { id = "b", text = "two" }]) };
	let (client_end, server_end) = duplex_pair();
	register_session(3, server_end, json_codec());
	let transport = local_rpc(board.dispatcher().into_protocol(json_codec()).for_connection(3));
	let client = BoardClient { transport, codec = json_codec(), reactive = ReactiveClient::new(client_end, json_codec()) };
	let first: KeyedSource<str, Row> = client.rows();
	let key_seen: Shared<str> = Shared::new("?");
	let whole = first.sub(|list| {});
	settle();
	let _key = first.sub_key("a", |row| key_seen.write() = text_of(row));
	settle();
	whole.dispose();
	settle();
	settle();
	print(i"key only: forwards={forwards()} a={key_seen.read()}");
	print(i"edit a:{client.edit("a", "uno").unwrap_or(0 - 1)}");
	settle();
	print(i"after: a={key_seen.read()} fault={fault_of(first.fault())}");
}
"#;

#[test]
fn a143_releasing_the_whole_lease_hands_a_held_key_back_without_revoking_the_channel() {
    let stdout = run_program("a143_handoff", A143_HANDOFF);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "key only: forwards=1 a=one",
            "edit a:0",
            "after: a=uno fault=none"
        ],
        "a key lease that outlives the whole lease must keep its channel; got:\n{stdout}"
    );
}

// --- A140: a retired mirror's route and replay are pruned ------------------

/// A140 IN PROCESS: five plain mints (five origins) and three keyed ones,
/// each leased and released for real, then one OLD handle leased again. Every
/// mint used to push a route and a replay onto the `ReactiveClient` that
/// nothing took out (`released: routes=8 replays=8`, the same after the old
/// handle's second release). Pruned on the mirror's retire hook (A134's
/// table event), and restored by `rebind` when a kept handle comes back.
const A140_PRUNE: &str = r#"import std::io::print;
import std::json::json_codec;
import std::hash_map::HashMap;
import std::reactive::{ Signal, SignalCell };
import std::rpc::{ KeyedCell, KeyedSource, ReactiveClient, RemoteSource, duplex_pair, local_rpc, register_session };
import std::shared::Shared;
import std::time::{ Duration, sleep_for };
import std::wire::{ Keyed, Wire };

[derive(Wire, PartialEq)]
struct Row {
	id: str,
	text: str,
}

impl Row with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

[service(BoardClient)]
struct Board {
	notes: Shared<HashMap<i32, SignalCell<str>>>,
	rows: KeyedCell<str, Row>,
}

impl Board {
	[rpc]
	fun write(self, id: i32, text: str): i32 {
		match self.notes.read().get(id) {
			Some(let cell) => cell.set(text),
			None => {},
		}
		0
	}

	[rpc]
	fun note(self, id: i32): Option<SignalCell<str>> {
		self.notes.read().get(id)
	}

	[rpc]
	fun rows(self, page: i32): KeyedCell<str, Row> {
		self.rows
	}
}

fun settle() {
	sleep_for(Duration::millis(0));
	sleep_for(Duration::millis(0));
	sleep_for(Duration::millis(0));
}

fun main() {
	let notes: HashMap<i32, SignalCell<str>> = HashMap::new();
	let board = Board { notes = Shared::new(notes), rows = KeyedCell::new([Row { id = "a", text = "one" }]) };
	mut id = 0;
	for id < 5 {
		board.notes.write().insert(id, Signal::new(i"n{id}"));
		id += 1;
	}
	let (client_end, server_end) = duplex_pair();
	register_session(3, server_end, json_codec());
	let transport = local_rpc(board.dispatcher().into_protocol(json_codec()).for_connection(3));
	let client = BoardClient { transport, codec = json_codec(), reactive = ReactiveClient::new(client_end, json_codec()) };
	print(i"start: routes={client.reactive.routes.read().len()} replays={client.reactive.replays.read().len()}");
	// N plain mints, each leased and released for real.
	mut kept: List<RemoteSource<str>> = [];
	mut round = 0;
	for round < 5 {
		let mirror: RemoteSource<str> = client.note(round);
		let lease = mirror.sub(|text| {});
		settle();
		lease.dispose();
		settle();
		kept.push(mirror);
		round += 1;
	}
	// N keyed mints (distinct origins), per-key leases released for real.
	round = 0;
	for round < 3 {
		let keyed: KeyedSource<str, Row> = client.rows(round);
		let lease = keyed.sub_key("a", |row| {});
		settle();
		lease.dispose();
		settle();
		round += 1;
	}
	print(i"released: routes={client.reactive.routes.read().len()} replays={client.reactive.replays.read().len()}");
	// A holder that kept an old handle can still lease it: it re-mints and follows.
	let old = kept.get(0).unwrap();
	let seen: Shared<str> = Shared::new("?");
	let again = old.sub(|text| seen.write() = text);
	settle();
	print(i"write:{client.write(0, "fresh").unwrap_or(0 - 1)}");
	settle();
	print(i"revived: seen={seen.read()} routes={client.reactive.routes.read().len()} replays={client.reactive.replays.read().len()}");
	again.dispose();
	settle();
	print(i"final: routes={client.reactive.routes.read().len()} replays={client.reactive.replays.read().len()}");
}
"#;

#[test]
fn a140_n_mints_then_n_releases_leave_no_routes_and_no_replays() {
    let stdout = run_program("a140_prune", A140_PRUNE);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "start: routes=0 replays=0",
            "released: routes=0 replays=0",
            "write:0",
            "revived: seen=fresh routes=1 replays=1",
            "final: routes=0 replays=0",
        ],
        "a retired mirror's route and replay must be pruned, and a kept handle \
         must come back on re-lease; got:\n{stdout}"
    );
}

/// A140 OVER A SOCKET: five plain mints over a real WebSocket, released for
/// real, then one old handle re-leased and released. Red before on `5`
/// routes and `5` replays in every line.
const A140_PRUNE_SOCKET: &str = r#"import std::io::print;
import std::json::json_codec;
import std::http::{ Response, Server };
import std::hash_map::HashMap;
import std::process::exit;
import std::reactive::{ Signal, SignalCell };
import std::result::Result::{ self, Ok, Err };
import std::rpc::RemoteSource;
import std::rpc::server::Service;
import std::shared::Shared;
import std::time::sleep;

[service(BoardClient)]
struct Board {
	notes: Shared<HashMap<i32, SignalCell<str>>>,
}

impl Board {
	[rpc]
	fun write(self, id: i32, text: str): i32 {
		match self.notes.read().get(id) {
			Some(let cell) => cell.set(text),
			None => {},
		}
		0
	}

	[rpc]
	fun note(self, id: i32): Option<SignalCell<str>> {
		self.notes.read().get(id)
	}
}

let board: Board = Board { notes = Shared::new(HashMap::new()) };

fun main() {
	mut id = 0;
	for id < 5 {
		board.notes.write().insert(id, Signal::new(i"n{id}"));
		id += 1;
	}
	Server::builder()
		.port(0)
		.with_service(Service::new(board.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

async fun until(ready: || bool) {
	mut tries = 0;
	for !ready() && tries < 300 {
		sleep(10);
		tries += 1;
	}
}

async fun run(port: i32) {
	match BoardClient::connect(i"ws://localhost:{port}/", json_codec()) {
		Ok(let client) => {
			let base = client.reactive.routes.read().len();
			mut kept: List<RemoteSource<str>> = [];
			mut round = 0;
			for round < 5 {
				let mirror: RemoteSource<str> = client.note(round);
				let lease = mirror.sub(|text| {});
				until(|| mirror.get().is_some());
				lease.dispose();
				until(|| mirror.released.read());
				kept.push(mirror);
				round += 1;
			}
			print(i"released: routes={client.reactive.routes.read().len() - base} replays={client.reactive.replays.read().len()}");
			let old = kept.get(0).unwrap();
			let seen: Shared<str> = Shared::new("?");
			let again = old.sub(|text| seen.write() = text);
			until(|| seen.read() == "n0");
			print(i"write:{client.write(0, "fresh").unwrap_or(0 - 1)}");
			until(|| seen.read() == "fresh");
			print(i"revived: seen={seen.read()} routes={client.reactive.routes.read().len() - base} replays={client.reactive.replays.read().len()}");
			again.dispose();
			until(|| old.released.read());
			print(i"final: routes={client.reactive.routes.read().len() - base} replays={client.reactive.replays.read().len()}");
		},
		Err(let error) => print(i"err:{error.debug()}"),
	}
	exit(0);
}
"#;

#[test]
fn a140_released_mirrors_leave_no_routes_over_a_socket() {
    let stdout = run_program("a140_prune_socket", A140_PRUNE_SOCKET);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "released: routes=0 replays=0",
            "write:0",
            "revived: seen=fresh routes=1 replays=1",
            "final: routes=0 replays=0",
        ],
        "over a socket, a retired mirror's route and replay must be pruned; \
         got:\n{stdout}"
    );
}

// --- A133: the in-process seed is INLINE, by contract -----------------------

/// A133 (ruled door c): a `duplex_pair` answers a mirror's `Subscribe` with its
/// seed INSIDE the send, so a node over a mirror that pulls as it subscribes
/// is told the seed twice in process: once through the pull, once as the
/// seed's own notification. Over a socket it is told once. This pins the
/// documented in-process contract (`duplex_pair`'s doc, the services guide),
/// so a change to it is a decision rather than a drift.
const A133_INLINE_SEED: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell, Source };
import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };
import std::time::{ Duration, sleep_for };

fun main() {
	let count: SignalCell<i32> = Signal::new(7);
	let (client_end, server_end) = duplex_pair();
	let server = ReactiveServer::new(server_end, json_codec());
	let channel = server.expose(count);
	let client = ReactiveClient::new(client_end, json_codec());
	let mirror: RemoteSource<i32> = client.attached_source(channel);
	let _watch = mirror.derive(|value| value.unwrap_or(0)).sub(|value| print(i"sees {value}"));
	sleep_for(Duration::millis(0));
	print("done");
}
"#;

#[test]
fn a133_an_in_process_seed_is_inline_and_a_node_over_the_mirror_sees_it_twice() {
    let stdout = run_program("a133_inline_seed", A133_INLINE_SEED);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec!["sees 7", "sees 7", "done"],
        "the in-process seed is inline by contract (A133 door c); got:\n{stdout}"
    );
}

// --- A142 S3: transient sources -------------------------------------------

/// A142 S3 IN PROCESS (`reactive-layers.md` §5, R4/R5/R13/R24/R27/R36/R37). One
/// program, six claims:
///
/// 1. `.transient()` over a flow of tasks: `Pending`, then `Ready`; a change moves
///    it to `Refreshing(v)`, where `get()` reads `None` and `latest()` keeps `v`
///    (R5, R27), and `is_pending()` is true. The LATEST task wins, by
///    cancellation: 2's slow task is started in the pipe's run and cancelled the
///    moment 3 is asked (it never finishes: `finished=2`).
/// 2. A failed refresh keeps the stale value: `Failed(e, Some(30))`, `get()` None,
///    `latest()` 30.
/// 3. OUT-OF-ORDER replies: tasks made outside the pipe belong to no run and are
///    never cancelled; the slow one answers LAST and is dropped (`Ready(60)`,
///    with 5 the last to finish).
/// 4. R36's bare-task arm: a panicking `Task<T>` is `Failed(message, stale)`,
///    `E = str`.
/// 5. `TaskSource` (R24): one task, `Pending` then `Ready`, or `Failed(e, None)`.
/// 6. A seal released with its owner applies no reply that lands after.
///
/// No stderr: a superseded task's abort is awaited by the seal, not reported.
const A142_S3_TRANSIENT: &str = r#"import std::io::{ panic, print };
import std::hash_map::HashMap;
import std::reactive::{ Flow, Owner, Pipe, Signal, SignalCell, Source, owner_scope };
import std::result::Result::{ self, Ok, Err };
import std::shared::Shared;
import std::task::Task;
import std::time::sleep;
import std::reactive::transient::{ TaskSource, Transient, TransientSource, TransientState };

fun show<E>(state: TransientState<i32, E>, error: |E| str): str {
	match state {
		TransientState::Pending => "Pending",
		TransientState::Ready(let v) => i"Ready({v})",
		TransientState::Refreshing(let v) => i"Refreshing({v})",
		TransientState::Failed(let e, let stale) => match stale {
			Some(let v) => i"Failed({error(e)}, {v})",
			None => i"Failed({error(e)}, -)",
		},
		TransientState::Absent => "Absent",
	}
}

fun text(state: TransientState<i32, str>): str {
	show(state, |e| e)
}

fun opt(value: Option<i32>): str {
	match value {
		Some(let v) => i"{v}",
		None => "-",
	}
}

/// Poll until `ready` holds — the program's own sequencing, never a bare sleep.
async fun until(ready: || bool) {
	mut tries = 0;
	for !ready() && tries < 300 {
		sleep(5);
		tries += 1;
	}
}

let finished: Shared<List<i32>> = Shared::new([]);

// Started INSIDE the pipe's body, so each is the run's and is cancelled with it.
async fun answer(x: i32, delay: i32): Result<i32, str> {
	sleep(delay);
	finished.write().push(x);
	if x == 4 {
		Err("four refused")
	} else {
		Ok(x * 10)
	}
}

// A bare task: its failure is a panic, and its message is the error (R36).
async fun bare(x: i32): i32 {
	sleep(5);
	if x == 2 {
		panic("two panicked");
	}
	x * 100
}

fun main() {
	let owner = Owner::new();
	let _run = async {
		// 1. Latest wins, by cancellation: 2 is slow and 3 is fast; 2's task is
		//    cancelled with its run the moment 3 is asked, and never finishes.
		let input: SignalCell<i32> = Signal::new(1);
		let seal: Transient<i32, str> = owner_scope.run(owner, || input.derive(|x| async answer(x, if x == 2 { 80 } else { 5 })).transient());
		let latest = owner_scope.run(owner, || seal.latest().memo());
		let pending = owner_scope.run(owner, || seal.is_pending().memo());
		print(i"start: {text(seal.state().get())} get={opt(seal.get())} latest={opt(latest.get())} pending={pending.get()}");
		until(|| seal.get().is_some());
		print(i"settled: {text(seal.state().get())} get={opt(seal.get())} latest={opt(latest.get())} pending={pending.get()}");
		input.set(2);
		// latest() across Refreshing: the old value stays, get() reads None.
		print(i"asked 2: {text(seal.state().get())} get={opt(seal.get())} latest={opt(latest.get())} pending={pending.get()}");
		input.set(3);
		until(|| seal.get() == Some(30));
		sleep(120);
		print(i"asked 3: {text(seal.state().get())} finished={finished.read().len()}");
		// 2. A failed refresh keeps the stale value.
		input.set(4);
		until(|| !pending.get());
		print(i"failed: {text(seal.state().get())} get={opt(seal.get())} latest={opt(latest.get())} pending={pending.get()}");

		// 3. Latest wins, by DROPPING: tasks made outside the pipe are nobody's
		//    run's, so nothing cancels them; the slow one's reply comes last and
		//    is dropped.
		let replies: Shared<HashMap<i32, Task<Result<i32, str>>>> = Shared::new(HashMap::new());
		replies.write().insert(5, async answer(5, 60));
		replies.write().insert(6, async answer(6, 5));
		let picked: SignalCell<i32> = Signal::new(5);
		let dropped: Transient<i32, str> = owner_scope.run(owner, || picked.derive(|x| replies.read().get(x).unwrap()).transient());
		picked.set(6);
		until(|| finished.read().len() == 5);
		print(i"out of order: {text(dropped.state().get())} finished-last={finished.read().get(4usize).unwrap_or(0)}");

		// 4. Bare tasks: E is str, the panic's message.
		let which: SignalCell<i32> = Signal::new(1);
		let plain: Transient<i32, str> = owner_scope.run(owner, || which.derive(|x| async bare(x)).transient());
		until(|| plain.get().is_some());
		which.set(2);
		until(|| !plain.state().get().is_pending());
		print(i"bare: {text(plain.state().get())}");

		// 5. One task.
		let one: TaskSource<i32, str> = TaskSource::new(async answer(7, 5));
		let one_bare: TaskSource<i32, str> = TaskSource::of(async bare(2));
		print(i"one: {text(one.state().get())} {text(one_bare.state().get())}");
		until(|| one.get().is_some() && !one_bare.state().get().is_pending());
		print(i"one: {text(one.state().get())} {text(one_bare.state().get())}");

		// 6. Released with its owner: the reply in flight is not applied.
		let gone = Owner::new();
		let later: SignalCell<i32> = Signal::new(8);
		let retired: Transient<i32, str> = owner_scope.run(gone, || later.derive(|x| replies.read().get(5).unwrap()).transient());
		gone.dispose();
		sleep(20);
		print(i"released: {text(retired.state().get())}");
		owner.dispose();
		print("done");
	};
}
"#;

#[test]
fn a142_s3_a_flow_of_tasks_is_a_transient_where_the_latest_task_wins() {
    let stdout = run_program("a142_s3_transient", A142_S3_TRANSIENT);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "start: Pending get=- latest=- pending=true",
            "settled: Ready(10) get=10 latest=10 pending=false",
            "asked 2: Refreshing(10) get=- latest=10 pending=true",
            "asked 3: Ready(30) finished=2",
            "failed: Failed(four refused, 30) get=- latest=30 pending=false",
            "out of order: Ready(60) finished-last=5",
            "bare: Failed(two panicked, 100)",
            "one: Pending Pending",
            "one: Ready(70) Failed(two panicked, -)",
            "released: Pending",
            "done",
        ],
        "a flow of tasks sealed with .transient(); got:\n{stdout}"
    );
}

/// A142 S3 OVER A SOCKET: a `RemoteSource` is a `TransientSource<T, RpcError>`
/// (§5's `Status` map). An `Option` handle the server answers `None` for is
/// `Absent`, one it answers is `Ready(v)` — and before anything watches either,
/// both are `Pending`, because `state()` reports and leases nothing. `latest()`
/// and `is_pending()` LEASE the mirror while bound, so binding them is enough to
/// mint; `latest()` follows a change.
const A142_S3_REMOTE_SOCKET: &str = r#"import std::io::print;
import std::json::json_codec;
import std::http::{ Response, Server };
import std::hash_map::HashMap;
import std::process::exit;
import std::reactive::{ Flow, Pipe, Signal, SignalCell, Source, Owner, owner_scope };
import std::result::Result::{ self, Ok, Err };
import std::rpc::{ RemoteSource, RpcError };
import std::rpc::server::Service;
import std::shared::Shared;
import std::time::sleep;
import std::reactive::transient::{ TransientSource, TransientState };

[service(BoardClient)]
struct Board {
	notes: Shared<HashMap<i32, SignalCell<str>>>,
}

impl Board {
	[rpc]
	fun write(self, id: i32, text: str): i32 {
		match self.notes.read().get(id) {
			Some(let cell) => cell.set(text),
			None => {},
		}
		0
	}

	[rpc]
	fun note(self, id: i32): Option<SignalCell<str>> {
		self.notes.read().get(id)
	}
}

let board: Board = Board { notes = Shared::new(HashMap::new()) };

fun main() {
	board.notes.write().insert(1, Signal::new("one"));
	Server::builder()
		.port(0)
		.with_service(Service::new(board.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

async fun until(ready: || bool) {
	mut tries = 0;
	for !ready() && tries < 300 {
		sleep(10);
		tries += 1;
	}
}

fun show(state: TransientState<str, RpcError>): str {
	match state {
		TransientState::Pending => "Pending",
		TransientState::Ready(let v) => i"Ready({v})",
		TransientState::Refreshing(let v) => i"Refreshing({v})",
		TransientState::Failed(let _e, let stale) => i"Failed({stale.unwrap_or("-")})",
		TransientState::Absent => "Absent",
	}
}

async fun run(port: i32) {
	match BoardClient::connect(i"ws://localhost:{port}/", json_codec()) {
		Ok(let client) => {
			let owner = Owner::new();
			let present: RemoteSource<str> = client.note(1);
			let missing: RemoteSource<str> = client.note(9);
			let present_state = owner_scope.run(owner, || present.state());
			let missing_state = owner_scope.run(owner, || missing.state());
			print(i"unwatched: present={show(present_state.get())} missing={show(missing_state.get())}");
			// Binding latest() leases the mirror.
			let present_latest = owner_scope.run(owner, || present.latest().memo());
			let missing_pending = owner_scope.run(owner, || missing.is_pending().memo());
			let missing_latest = owner_scope.run(owner, || missing.latest().memo());
			until(|| present_latest.get().is_some() && !missing_pending.get());
			print(i"watched: present={show(present_state.get())} latest={present_latest.get().unwrap_or("-")} missing={show(missing_state.get())} missing-pending={missing_pending.get()} missing-latest={missing_latest.get().unwrap_or("-")}");
			print(i"write:{client.write(1, "uno").unwrap_or(0 - 1)}");
			until(|| present_latest.get() == Some("uno"));
			print(i"after: present={show(present_state.get())} latest={present_latest.get().unwrap_or("-")} get={present.get().unwrap_or("-")}");
			owner.dispose();
		},
		Err(let error) => print(i"err:{error.debug()}"),
	}
	exit(0);
}
"#;

#[test]
fn a142_s3_a_mirror_is_a_transient_absent_versus_pending_over_a_socket() {
    let stdout = run_program("a142_s3_remote_socket", A142_S3_REMOTE_SOCKET);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "unwatched: present=Pending missing=Pending",
            "watched: present=Ready(one) latest=one missing=Absent missing-pending=false missing-latest=-",
            "write:0",
            "after: present=Ready(uno) latest=uno get=uno",
        ],
        "over a socket, a mirror's transient state; got:\n{stdout}"
    );
}

// --- A150: `states()`, the whole state LEASED (R-c door (b)) ---------------

/// A150 OVER A SOCKET: `state()` stays a passive report (an unwatched mirror
/// reads `Pending` forever), and `states()` is a pipe of the whole state that
/// LEASES the mirror while its consumer holds it — so binding it alone moves a
/// present mirror to `Ready` and a missing one to `Absent`, which is what tells
/// a spinner from "not found". `TransientState::map` reads through it.
const A150_STATES_SOCKET: &str = r#"import std::io::print;
import std::debug::Debug;
import std::json::json_codec;
import std::http::{ Response, Server };
import std::hash_map::HashMap;
import std::process::exit;
import std::reactive::{ Flow, Pipe, Signal, SignalCell, Source, Owner, owner_scope };
import std::result::Result::{ self, Ok, Err };
import std::rpc::{ RemoteSource, RpcError };
import std::rpc::server::Service;
import std::shared::Shared;
import std::time::sleep;
import std::reactive::transient::{ TransientSource, TransientState };

[service(BoardClient)]
struct Board {
	notes: Shared<HashMap<i32, SignalCell<str>>>,
}

impl Board {
	[rpc]
	fun write(self, id: i32, text: str): i32 {
		match self.notes.read().get(id) {
			Some(let cell) => cell.set(text),
			None => {},
		}
		0
	}

	[rpc]
	fun note(self, id: i32): Option<SignalCell<str>> {
		self.notes.read().get(id)
	}
}

let board: Board = Board { notes = Shared::new(HashMap::new()) };

fun main() {
	board.notes.write().insert(1, Signal::new("one"));
	Server::builder()
		.port(0)
		.with_service(Service::new(board.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

async fun until(ready: || bool) {
	mut tries = 0;
	for !ready() && tries < 300 {
		sleep(10);
		tries += 1;
	}
}

fun count(n: usize): str {
	i"{n}"
}

fun show<T>(state: TransientState<T, RpcError>, text: |T| str): str {
	match state {
		TransientState::Pending => "Pending",
		TransientState::Ready(let v) => i"Ready({text(v)})",
		TransientState::Refreshing(let v) => i"Refreshing({text(v)})",
		TransientState::Failed(let _e, let _stale) => "Failed",
		TransientState::Absent => "Absent",
	}
}

async fun run(port: i32) {
	match BoardClient::connect(i"ws://localhost:{port}/", json_codec()) {
		Ok(let client) => {
			let owner = Owner::new();
			let present: RemoteSource<str> = client.note(1);
			let missing: RemoteSource<str> = client.note(9);
			// The passive report, and nothing else watching: it stays Pending.
			let reported = owner_scope.run(owner, || present.state());
			sleep(50);
			print(i"reported: {show(reported.get(), |v| v)}");
			// `states()` alone leases: Ready for the present note, Absent for the
			// missing one — and `map` reads through the state.
			let present_states = owner_scope.run(owner, || present
				.states()
				.derive(|state| state.map(|text| text.len()))
				.memo());
			let missing_states = owner_scope.run(owner, || missing.states().memo());
			until(|| !present_states.get().is_pending() && !missing_states.get().is_pending());
			print(i"watched: present={show(present_states.get(), count)} missing={show(missing_states.get(), |v| v)}");
			print(i"write:{client.write(1, "three").unwrap_or(0 - 1)}");
			until(|| present_states.get().latest() == Some(5));
			print(i"after: present={show(present_states.get(), count)}");
			owner.dispose();
		},
		Err(let error) => print(i"err:{error.debug()}"),
	}
	exit(0);
}
"#;

#[test]
fn a150_states_leases_a_mirror_and_state_stays_a_passive_report() {
    let stdout = run_program("a150_states_socket", A150_STATES_SOCKET);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "reported: Pending",
            "watched: present=Ready(3) missing=Absent",
            "write:0",
            "after: present=Ready(5)",
        ],
        "states() must lease the mirror and report every arm; got:\n{stdout}"
    );
}

// --- A145: the read-only seal crosses as a handle ----------------------------

/// A145 OVER A SOCKET: `MemoCell<T>` — the read-only seal of a derivation (A142
/// R20) — and `Option<MemoCell<T>>` are `[rpc]` handle returns, exactly like
/// `SignalCell<T>`: the route exports the cell behind the memo, and the client
/// mints a `RemoteSource<T>`. Before, both were refused as "not Wire" and the
/// stubs typed as the raw return.
const A145_MEMO_HANDLE_SOCKET: &str = r#"import std::io::print;
import std::json::json_codec;
import std::http::{ Response, Server };
import std::hash_map::HashMap;
import std::process::exit;
import std::reactive::{ MemoCell, Pipe, Signal, SignalCell, Source };
import std::result::Result::{ self, Ok, Err };
import std::rpc::RemoteSource;
import std::rpc::server::Service;
import std::shared::Shared;
import std::time::sleep;

// A server derivation exposed READ-ONLY: the handle method answers the sealed
// memo, which lives as long as the service (made once, at the top).
[service(CounterClient)]
struct Counter {
	count: SignalCell<i32>,
	doubled: MemoCell<i32>,
	labels: Shared<HashMap<i32, MemoCell<str>>>,
}

impl Counter {
	[rpc]
	fun bump(self, to: i32): i32 {
		self.count.set(to);
		to
	}

	[rpc]
	fun doubled(self): MemoCell<i32> {
		self.doubled
	}

	[rpc]
	fun label(self, id: i32): Option<MemoCell<str>> {
		self.labels.read().get(id)
	}
}

let count: SignalCell<i32> = Signal::new(1);
let counter: Counter = Counter {
	count,
	doubled = count.derive(|x| x * 2).memo_global(),
	labels = Shared::new(HashMap::new()),
};

fun main() {
	counter.labels.write().insert(1, count.derive(|x| i"n{x}").memo_global());
	Server::builder()
		.port(0)
		.with_service(Service::new(counter.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

async fun until(ready: || bool) {
	mut tries = 0;
	for !ready() && tries < 300 {
		sleep(10);
		tries += 1;
	}
}

async fun run(port: i32) {
	match CounterClient::connect(i"ws://localhost:{port}/", json_codec()) {
		Ok(let client) => {
			let doubled: RemoteSource<i32> = client.doubled();
			let label: RemoteSource<str> = client.label(1);
			let missing: RemoteSource<str> = client.label(9);
			let _a = doubled.sub(|value| {});
			let _b = label.sub(|value| {});
			let _c = missing.sub(|value| {});
			until(|| doubled.get().is_some() && label.get().is_some());
			print(i"seed: doubled={doubled.get().unwrap_or(0 - 1)} label={label.get().unwrap_or("-")} missing={missing.status().get().debug()}");
			print(i"bump:{client.bump(5).unwrap_or(0 - 1)}");
			until(|| doubled.get() == Some(10));
			print(i"after: doubled={doubled.get().unwrap_or(0 - 1)} label={label.get().unwrap_or("-")}");
		},
		Err(let error) => print(i"err:{error.debug()}"),
	}
	exit(0);
}
"#;

#[test]
fn a145_a_memo_cell_is_an_rpc_handle_return_over_a_socket() {
    let stdout = run_program("a145_memo_handle_socket", A145_MEMO_HANDLE_SOCKET);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "seed: doubled=2 label=n1 missing=Status::Absent",
            "bump:5",
            "after: doubled=10 label=n5"
        ],
        "a MemoCell handle must cross as a mirror; got:\n{stdout}"
    );
}

// --- A138: a map's per-key handle crosses as a mirror -------------------------

/// A138: an `[rpc]` method answering `HashMapCell::at(key)` — a `HashMapEntry<K, V>` —
/// replies with a plain channel, `RemoteSource<Option<V>>` at the client (the
/// mirror a `MemoCell<Option<V>>` reply makes, so the contract does not move),
/// and the forward rides the key's SLOT: a post to key 1 sends nothing to the
/// mirror of key 2.
const A138_MAP_ENTRY_SOCKET: &str = r#"import std::io::print;
import std::json::json_codec;
import std::http::{ Response, Server };
import std::hash_map::HashMap;
import std::process::exit;
import std::reactive::{ HashMapCell, HashMapEntry, Source };
import std::result::Result::{ self, Ok, Err };
import std::rpc::RemoteSource;
import std::rpc::server::Service;
import std::shared::Shared;
import std::time::sleep;

[service(InboxClient)]
struct Inbox {
	messages: HashMapCell<i32, str>,
}

impl Inbox {
	[rpc]
	fun post(self, id: i32, text: str): i32 {
		self.messages.insert(id, text);
		id
	}

	[rpc]
	fun message(self, id: i32): HashMapEntry<i32, str> {
		self.messages.at(id)
	}
}

fun main() {
	let inbox = Inbox { messages = HashMapCell::of([(1, "hello")].to_map()) };
	Server::builder()
		.port(0)
		.with_service(Service::new(inbox.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

async fun until(ready: || bool) {
	mut tries = 0;
	for !ready() && tries < 300 {
		sleep(10);
		tries += 1;
	}
}

async fun run(port: i32) {
	match InboxClient::connect(i"ws://localhost:{port}/", json_codec()) {
		Ok(let client) => {
			let first: RemoteSource<Option<str>> = client.message(1);
			let second: RemoteSource<Option<str>> = client.message(2);
			let second_updates = Shared::new(0);
			let _a = first.sub(|value| {});
			let _b = second.sub(|value| second_updates.write() += 1);
			until(|| first.get().is_some() && second.get().is_some());
			print(i"seed: first={first.get().unwrap_or(None).unwrap_or("-")} second={second.get().unwrap_or(None).unwrap_or("-")}");
			let seeded = second_updates.read();
			print(i"post:{client.post(1, "edited").unwrap_or(0 - 1)}");
			until(|| first.get() == Some(Some("edited")));
			print(i"post:{client.post(2, "new").unwrap_or(0 - 1)}");
			until(|| second.get() == Some(Some("new")));
			print(i"after: first={first.get().unwrap_or(None).unwrap_or("-")} second={second.get().unwrap_or(None).unwrap_or("-")} second_updates={second_updates.read() - seeded}");
		},
		Err(let error) => print(i"err:{error.debug()}"),
	}
	exit(0);
}
"#;

#[test]
fn a138_a_map_entry_is_an_rpc_handle_return_over_a_socket() {
    let stdout = run_program("a138_map_entry_socket", A138_MAP_ENTRY_SOCKET);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "seed: first=hello second=-",
            "post:1",
            "post:2",
            "after: first=edited second=new second_updates=1"
        ],
        "a HashMapEntry handle must cross as a per-key mirror; got:\n{stdout}"
    );
}

// --- A135: a handler runs under its CONNECTION's owner ----------------------

/// A135 IN PROCESS: kolt's shape — a handle method whose body is
/// `self.channels.map(..).cell()` — called three times (three leases, each
/// released for real, so three mints), then the connection's session dropped.
/// The store cell's live registrations are counted directly.
const A135_CONNECTION_OWNER: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Signal, SignalCell, Source };
import std::rpc::{ ReactiveClient, RemoteSource, drop_session, duplex_pair, local_rpc, register_session };
import std::shared::Shared;
import std::time::{ Duration, sleep_for };

[service(StoreClient)]
struct Store {
	channels: SignalCell<List<i32>>,
	calls: Shared<i32>,
}

impl Store {
	// The shape A135 found in kolt's store: a derived cell minted per CALL.
	[rpc]
	fun get_channels(self): SignalCell<List<i32>> {
		self.calls.write() += 1;
		self.channels.derive(|ids| ids).cell()
	}
}

/// The registrations standing on a cell's subscriber list that are still live.
fun live_on<T>(cell: SignalCell<T>): i32 {
	mut live = 0;
	for subscriber in cell.subscribers.read() {
		if subscriber.live.read() {
			live += 1;
		}
	}
	live
}

fun main() {
	let channels: SignalCell<List<i32>> = Signal::new([1, 2]);
	let store = Store { channels, calls = Shared::new(0) };
	// The app's own watcher: the one registration that must survive.
	let _app = channels.sub(|ids| {});
	print(i"baseline:{live_on(channels)}");
	let (client_end, server_end) = duplex_pair();
	register_session(9, server_end, json_codec());
	let transport = local_rpc(store.dispatcher().into_protocol(json_codec()).for_connection(9));
	let client = StoreClient { transport, codec = json_codec(), reactive = ReactiveClient::new(client_end, json_codec()) };
	// Three leases, each released for real before the next: three mints, so
	// three CALLS, each minting its own derived cell on the server.
	mut round = 0;
	for round < 3 {
		let mirror: RemoteSource<List<i32>> = client.get_channels();
		let lease = mirror.sub(|ids| {});
		sleep_for(Duration::millis(0));
		lease.dispose();
		sleep_for(Duration::millis(0));
		sleep_for(Duration::millis(0));
		round += 1;
	}
	print(i"connected:calls={store.calls.read()} live={live_on(channels)}");
	// The connection goes: its session, and with A135 its OWNER, are disposed.
	drop_session(9);
	print(i"disconnected:live={live_on(channels)}");
}
"#;

/// A135 (door b): the dispatcher runs each handler under the connection's owner
/// (`rpc::under_connection`, written into every generated route), and
/// `ReactiveServer::dispose` — which `drop_session` runs — disposes it. A
/// per-call `.cell()` used to keep its registration on the store's cell for
/// the life of the PROCESS: `disconnected:live=4` before (the app's own watcher
/// plus one per call), `1` after — ONE registration, the app's. While the
/// connection lives the three stand (`connected:… live=4`): door (b) bounds the
/// leak by the connection and does not restore the dedup, which is what the
/// compiler's warning at that `.cell()` steers to (door c, pinned in
/// `vilan-core`'s `inference::lifetimes`).
#[test]
fn a135_n_calls_of_a_per_call_cell_leave_one_registration_after_disconnect() {
    let stdout = run_program_warning(
        "a135_owner",
        A135_CONNECTION_OWNER,
        "`get_channels` returns a signal handle it builds with `.cell()`",
    );
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "baseline:1",
            "connected:calls=3 live=4",
            "disconnected:live=1",
        ],
        "a handler's per-call cell must be released with its connection; got:\n{stdout}"
    );
}

/// A135 OVER A SOCKET: the same three calls over a real WebSocket, and the
/// disconnect the real way — the client's socket closes, and the server's
/// teardown drops the connection's session (and its owner). A
/// `Service::factory` service: one instance per connection, so its handlers
/// run under the connection's owner. (Under `Service::new` the one shared
/// instance's handlers run under the SERVICE's owner since A141, and a
/// per-call cell lives with the service — the A141 pins below.)
const A135_CONNECTION_OWNER_SOCKET: &str = r#"import std::io::print;
import std::json::json_codec;
import std::http::{ Response, Server };
import std::process::exit;
import std::reactive::{ Signal, SignalCell, Source };
import std::result::Result::{ self, Ok, Err };
import std::rpc::RemoteSource;
import std::rpc::server::Service;
import std::shared::Shared;
import std::time::sleep;

[service(StoreClient)]
struct Store {
	channels: SignalCell<List<i32>>,
	calls: Shared<i32>,
}

impl Store {
	[rpc]
	fun get_channels(self): SignalCell<List<i32>> {
		self.calls.write() += 1;
		self.channels.derive(|ids| ids).cell()
	}
}

let channels: SignalCell<List<i32>> = Signal::new([1, 2]);
let store: Store = Store { channels, calls = Shared::new(0) };

fun live_on<T>(cell: SignalCell<T>): i32 {
	mut live = 0;
	for subscriber in cell.subscribers.read() {
		if subscriber.live.read() {
			live += 1;
		}
	}
	live
}

fun main() {
	Server::builder()
		.port(0)
		.with_service(Service::factory(|connection| store, json_codec()))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

/// Poll until `ready` holds — the harness's own sequencing, never a bare sleep.
async fun until(ready: || bool) {
	mut tries = 0;
	for !ready() && tries < 1000 {
		sleep(10);
		tries += 1;
	}
}

async fun run(port: i32) {
	let _app = channels.sub(|ids| {});
	print(i"baseline:{live_on(channels)}");
	match StoreClient::connect(i"ws://localhost:{port}/", json_codec()) {
		Ok(let client) => {
			mut round = 0;
			for round < 3 {
				let mirror: RemoteSource<List<i32>> = client.get_channels();
				let lease = mirror.sub(|ids| {});
				until(|| mirror.get().is_some());
				lease.dispose();
				// The close is real once the settle and the hop have passed and
				// the server has revoked the channel.
				until(|| !mirror.minted() || mirror.released.read());
				round += 1;
			}
			until(|| store.calls.read() == 3);
			print(i"connected:calls={store.calls.read()} live={live_on(channels)}");
			// The client goes away for good: the server's teardown drops the
			// connection's session, and with A135 its owner.
			client.transport.duplex.socket.read().close();
			until(|| live_on(channels) == 1);
			print(i"disconnected:live={live_on(channels)}");
		},
		Err(let error) => print(i"err:{error.debug()}"),
	}
	exit(0);
}
"#;

#[test]
fn a135_a_closed_socket_releases_its_handlers_per_call_cells() {
    let stdout = run_program_warning(
        "a135_owner_socket",
        A135_CONNECTION_OWNER_SOCKET,
        "`get_channels` returns a signal handle it builds with `.cell()`",
    );
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "baseline:1",
            "connected:calls=3 live=4",
            "disconnected:live=1",
        ],
        "over a socket, a handler's per-call cell must be released when the \
         connection closes; got:\n{stdout}"
    );
}

// --- A141: a `Service::new` handler runs under the SERVICE's owner ----------

/// A141 OVER A SOCKET (R-f, door b): kolt's `get_user` shape on a
/// `Service::new` service — a handle method that caches a derived `.cell()`
/// in a `Memo` on the ONE shared store. Three connections, one after
/// another; each subscribes, bumps the source, reads the derivation, and
/// closes. Under A135's per-connection owner the cached cell died with the
/// FIRST connection and the store kept handing it out: `doubled=20` for
/// every later connection (red before on `conn 1`/`conn 2`). Under the
/// service's owner it survives every close.
const A141_SERVICE_OWNER_SOCKET: &str = r#"import std::io::print;
import std::json::json_codec;
import std::http::{ Response, Server };
import std::memo::Memo;
import std::process::exit;
import std::reactive::{ Signal, SignalCell, Source };
import std::result::Result::{ self, Ok, Err };
import std::rpc::RemoteSource;
import std::rpc::server::Service;
import std::shared::Shared;
import std::time::sleep;

[service(StoreClient)]
struct Store {
	source: SignalCell<i32>,
	cache: Memo<i32, SignalCell<i32>>,
}

impl Store {
	[rpc]
	fun bump(self, to: i32): i32 {
		self.source.set(to);
		to
	}

	// Cached on the shared store: the first call builds it, every later call
	// (any connection's) is answered with the same cell.
	[rpc]
	fun doubled(self): SignalCell<i32> {
		self.cache.get_or_insert(0, || self.source.derive(|x| x * 2).cell())
	}
}

let source: SignalCell<i32> = Signal::new(1);
let store: Store = Store { source, cache = Memo::new() };

fun main() {
	Server::builder()
		.port(0)
		.with_service(Service::new(store.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

/// Poll until `ready` holds — the harness's own sequencing, never a bare sleep.
async fun until(ready: || bool) {
	mut tries = 0;
	for !ready() && tries < 300 {
		sleep(10);
		tries += 1;
	}
}

fun shown(value: Option<i32>): i32 {
	match value {
		Some(let held) => held,
		None => 0 - 1,
	}
}

async fun run(port: i32) {
	mut round = 0;
	for round < 3 {
		match StoreClient::connect(i"ws://localhost:{port}/", json_codec()) {
			Ok(let client) => {
				let mirror: RemoteSource<i32> = client.doubled();
				let _lease = mirror.sub(|value| {});
				until(|| mirror.get().is_some());
				let want = (round + 1) * 10;
				let bumped = client.bump(want).unwrap_or(0 - 1);
				until(|| shown(mirror.get()) == want * 2);
				print(i"conn {round}: bump={bumped} doubled={shown(mirror.get())}");
				// This connection goes away for good before the next one comes.
				client.transport.duplex.socket.read().close();
				sleep(50);
			},
			Err(let error) => print(i"err:{error.debug()}"),
		}
		round += 1;
	}
	exit(0);
}
"#;

#[test]
fn a141_a_service_new_handlers_cached_cell_survives_every_close_over_a_socket() {
    let stdout = run_program_warning(
        "a141_service_owner_socket",
        A141_SERVICE_OWNER_SOCKET,
        "inside a `Memo` maker ties what it builds",
    );
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "conn 0: bump=10 doubled=20",
            "conn 1: bump=20 doubled=40",
            "conn 2: bump=30 doubled=60",
        ],
        "a derivation a `Service::new` handler caches on the shared store must \
         survive the connection that built it; got:\n{stdout}"
    );
}

/// A141 IN PROCESS: the same store, two sessions on two `duplex_pair`s, and
/// the instance-owner line drawn both ways over ONE shared `Store` value.
/// `shared` is a protocol stamped with a service owner (what `Service::new`
/// stamps, `RpcProtocol::under_owner`); `per_connection` is left unstamped
/// (what `Service::factory` leaves), so its handlers run under the
/// connection's owner. Connection 1 builds each cache and is dropped;
/// connection 2 is then answered from the cache. The stamped cache keeps
/// following (`shared:2=6`); the unstamped one was built under connection 1's
/// owner and is dead (`per_connection:2=3`, never 6) — the case the compiler's A141
/// warning steers to `.cell_global()`.
const A141_SERVICE_OWNER: &str = r#"import std::io::print;
import std::json::json_codec;
import std::reactive::{ Owner, Signal, SignalCell, Source };
import std::rpc::{ ReactiveClient, RemoteSource, drop_session, duplex_pair, local_rpc, register_session };
import std::shared::Shared;
import std::time::{ Duration, sleep_for };

[service(StoreClient)]
struct Store {
	source: SignalCell<i32>,
	slot: Shared<Option<SignalCell<i32>>>,
}

impl Store {
	[rpc]
	fun tripled(self): SignalCell<i32> {
		self.slot.get_or_insert(|| self.source.derive(|x| x * 3).cell())
	}
}

fun shown(value: Option<i32>): i32 {
	match value {
		Some(let held) => held,
		None => 0 - 1,
	}
}

fun settle() {
	sleep_for(Duration::millis(0));
	sleep_for(Duration::millis(0));
}

/// Connection 1 builds the cache and goes; connection 2 reads it after a bump.
fun twice(label: str, store: Store, stamp: bool) {
	mut connection = 1;
	for connection <= 2 {
		let id = if stamp { 20 + connection } else { 30 + connection };
		let (client_end, server_end) = duplex_pair();
		register_session(id, server_end, json_codec());
		let base = store.dispatcher().into_protocol(json_codec()).for_connection(id);
		let protocol = if stamp { base.under_owner(Owner::new()) } else { base };
		let client = StoreClient { transport = local_rpc(protocol), codec = json_codec(), reactive = ReactiveClient::new(client_end, json_codec()) };
		let mirror: RemoteSource<i32> = client.tripled();
		let lease = mirror.sub(|value| {});
		settle();
		if connection == 2 {
			store.source.set(2);
			settle();
			print(i"{label}:2={shown(mirror.get())}");
		} else {
			print(i"{label}:1={shown(mirror.get())}");
		}
		lease.dispose();
		settle();
		drop_session(id);
		connection += 1;
	}
}

fun main() {
	let shared_store = Store { source = Signal::new(1), slot = Shared::new(None) };
	twice("shared", shared_store, true);
	let per_connection = Store { source = Signal::new(1), slot = Shared::new(None) };
	twice("per_connection", per_connection, false);
}
"#;

#[test]
fn a141_a_stamped_instance_owner_keeps_a_cached_cell_past_its_connection() {
    let stdout = run_program_warning(
        "a141_service_owner",
        A141_SERVICE_OWNER,
        "stored on a structure that outlives the call",
    );
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "shared:1=3",
            "shared:2=6",
            "per_connection:1=3",
            "per_connection:2=3",
        ],
        "a stamped (Service::new) owner keeps the cache alive; an unstamped \
         (Service::factory) one releases it with its connection; got:\n{stdout}"
    );
}

// --- A136: what a Memo maker builds outlives the caller ---------------------

/// A136 IN PROCESS: the order's kolt-mirrors probe, sections F and G, on
/// `get_or_insert` (I7). F memoizes an owner-tied `.cell()`; G memoizes the
/// program-lifetime cell of the unleased mirror and leases at the call site.
const A136_MEMO_MAKER: &str = r#"import std::io::print;
import std::json::json_codec;
import std::hash_map::HashMap;
import std::memo::Memo;
import std::reactive::{ Owner, Signal, SignalCell, Source, owner_scope };
import std::rpc::{ LocalTransport, ReactiveClient, RemoteSource, duplex_pair, local_rpc, register_session };
import std::shared::Shared;
import std::time::{ Duration, sleep_for };

[service(Client)]
struct Store {
	messages: Shared<HashMap<i32, SignalCell<List<str>>>>,
}

impl Store {
	[rpc]
	fun add_message(self, id: i32, text: str): bool {
		match self.messages.read().get(id) {
			Some(let list) => {
				list.update(|&mut items| {
					items.push(text);
				});
				true
			},
			None => false,
		}
	}

	[rpc]
	fun get_messages(self, id: i32): Option<SignalCell<List<str>>> {
		self.messages.read().get(id)
	}
}

// F — the trap: the maker builds an owner-tied `.cell()` under the FIRST
// caller's owner, and the memo keeps it after that owner is gone.
let handles_f: Memo<i32, SignalCell<Option<List<str>>>> = Memo::new();

fun messages_f(client_cell: SignalCell<Option<Client<LocalTransport>>>, id: i32): SignalCell<List<str>> {
	handles_f
		.get_or_insert(id, || client_cell
			.derive(|client| client.map(|client| client.get_messages(id)))
			.cell()
			.and_then(|mirror| mirror)
			.cell())
		.derive(|x| x.unwrap_or_default())
		.cell()
}

// G — the rule: the memo keeps a program-lifetime cell of the (unleased)
// mirror, and the lease is the caller's.
let handles_g: Memo<i32, SignalCell<Option<RemoteSource<List<str>>>>> = Memo::new();

fun messages_g(client_cell: SignalCell<Option<Client<LocalTransport>>>, id: i32): SignalCell<List<str>> {
	handles_g
		.get_or_insert(id, || client_cell
			.derive(|client| client.map(|client| client.get_messages(id)))
			.cell_global())
		.and_then(|mirror| mirror)
		.derive(|x| x.unwrap_or_default())
		.cell()
}

fun main() {
	let first: SignalCell<List<str>> = Signal::new(["a"]);
	let messages: HashMap<i32, SignalCell<List<str>>> = HashMap::new();
	let store = Store { messages = Shared::new(messages) };
	store.messages.write().insert(0, first);
	let (client_end, server_end) = duplex_pair();
	register_session(4, server_end, json_codec());
	let transport = local_rpc(store.dispatcher().into_protocol(json_codec()).for_connection(4));
	let client = Client { transport, codec = json_codec(), reactive = ReactiveClient::new(client_end, json_codec()) };
	let client_cell: SignalCell<Option<Client<LocalTransport>>> = Signal::new(Some(client));

	let visit1 = Owner::new();
	owner_scope.run(visit1, || {
		messages_g(client_cell, 0).effect(|x| print(i"G visit 1 sees {x.len()}"));
		messages_f(client_cell, 0).effect(|x| print(i"F visit 1 sees {x.len()}"));
	});
	sleep_for(Duration::millis(0));
	print(i"add -> {client.add_message(0, "b").unwrap_or(false)}");
	print("(leave)");
	visit1.dispose();
	sleep_for(Duration::millis(0));
	sleep_for(Duration::millis(0));
	let visit2 = Owner::new();
	owner_scope.run(visit2, || {
		messages_f(client_cell, 0).effect(|x| print(i"F visit 2 sees {x.len()}"));
		messages_g(client_cell, 0).effect(|x| print(i"G visit 2 sees {x.len()}"));
	});
	sleep_for(Duration::millis(0));
	print(i"add -> {client.add_message(0, "c").unwrap_or(false)}");
	visit2.dispose();
	print("done");
}
"#;

/// A136: F's maker builds a `.cell()` under the FIRST visit's owner and the
/// memo keeps it past that owner — visit 2 is handed the dead cell and reads
/// `2` forever (`c` never arrives). G's maker builds `.cell_global()` and the
/// lease is the caller's: visit 2 paints the cached `2`, re-mints, and follows
/// the add to `3`. F's two `.cell()`s are also what the compiler now warns at
/// (door b), which this build pins through the real pipeline.
#[test]
fn a136_a_memo_maker_that_builds_cell_global_survives_its_first_caller() {
    let stdout = run_program_warning(
        "a136_memo",
        A136_MEMO_MAKER,
        "`.cell()` inside a `Memo` maker ties what it builds to the FIRST caller's owner",
    );
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "G visit 1 sees 0",
            "F visit 1 sees 0",
            "G visit 1 sees 1",
            "F visit 1 sees 1",
            "G visit 1 sees 2",
            "F visit 1 sees 2",
            "add -> true",
            "(leave)",
            // F: the dead cell, never updated again.
            "F visit 2 sees 2",
            // G: the cached value, the re-mint's seed, then the add.
            "G visit 2 sees 2",
            "G visit 2 sees 2",
            "G visit 2 sees 3",
            "add -> true",
            "done",
        ],
        "a memo maker's cell must outlive its first caller only when it is \
         `.cell_global()`; got:\n{stdout}"
    );
}

// --- A153 S1: the mirrored store's server half (`mirrored-store.md` §3–§5) ---

/// A server's `Store<Global>` mirrored over one connection, the replies wired by
/// hand through `std::rpc::mirror`'s repliers (what the `[service]` expansion
/// writes for a `Store`/`StoreSome` return), the client end reading raw frames.
const MIRROR_WIRE: &str = r##"import std::hash_map::HashMap;
import std::io::print;
import std::json::json_codec;
import std::option::Option::{ self, None, Some };
import std::reactive::{ SequenceCell, batch };
import std::reactive::store::{ Storable, Store, store_census, store_wire_census };
import std::rpc::{ Dispatcher, DuplexTransport, RpcRequest, drop_session, duplex_pair, encode_request, local_rpc, register_session };
import std::rpc::mirror::{ reply_store, reply_store_some };
import std::wire::{ Frame, Wire };

[derive(Storable, Wire)]
struct Message {
	id: u53,
	author: str,
	content: str,
}

[derive(Storable, Wire)]
struct Room {
	id: u53,
	name: str,
	messages: List<u53>,
}

[derive(Storable, Wire)]
struct Global {
	rooms: HashMap<u53, Room>,
	messages: HashMap<u53, Message>,
	motd: str,
}

fun text(frame: Frame): str {
	match frame {
		Frame::Text(let value) => value,
		Frame::Binary(let _bytes) => "<binary>",
	}
}

fun census(label: str, global: Store<Global>) {
	let (slots, nodes) = store_census(global);
	print(i"{label}: slots={slots} nodes={nodes} wire={store_wire_census(global)}");
}

fun main() {
	let codec = json_codec();
	mut rooms: HashMap<u53, Room> = HashMap::new();
	rooms.insert(0, Room { id = 0, name = "general", messages = [7] });
	mut messages: HashMap<u53, Message> = HashMap::new();
	messages.insert(7, Message { id = 7, author = "bob", content = "hello" });
	messages.insert(8, Message { id = 8, author = "amy", content = "hi" });
	let global = Store::new(Global { rooms, messages, motd = "welcome" });
	census("before", global);
	let (client_end, server_end) = duplex_pair();
	register_session(7, server_end, codec);
	client_end.on_frame(|frame| print(i"down {text(frame)}"));
	let dispatcher = Dispatcher::new()
		.on("global", |request: RpcRequest| reply_store(request, global))
		.on("message", |request: RpcRequest| reply_store_some(request, global.messages().at(7).some()));
	let local = local_rpc(dispatcher.into_protocol(codec).for_connection(7));
	print(i"reply {text((local.handler)(encode_request(codec, "global", [])))}");
	print(i"reply {text((local.handler)(encode_request(codec, "message", [])))}");
	census("granted", global);
	print("-- a write inside the root's boundary");
	global.motd().set("bye");
	print("-- a write inside the message's boundary");
	let _edited = global.messages().at(7).some().content().patch("goodbye");
	print("-- a write under a key nobody watches");
	let _other = global.messages().at(8).some().content().patch("ho");
	print("-- two slots in one Subscribe");
	client_end.send(Frame::Text("{\"Subscribe\":[0,[[0,-1,[0,0]],[1,-1,[1,8]]]]}"));
	census("subscribed", global);
	print("-- one turn, many writes");
	batch(|| {
		let _renamed = global.rooms().at(0).some().name().patch("lobby");
		global.rooms().at(0).some().messages().push(9);
		let _again = global.rooms().at(0).some().name().patch("lounge");
		global.motd().set("one turn");
		let _content = global.messages().at(8).some().content().patch("x");
		let _author = global.messages().at(8).some().author().patch("zed");
	});
	print("-- a write above a boundary covers the writes inside it");
	batch(|| {
		let _inside = global.messages().at(7).some().content().patch("inside");
		global.messages().at(7).set(Some(Message { id = 7, author = "bob", content = "whole" }));
	});
	print("-- the same value");
	global.motd().set("one turn");
	global.messages().at(7).set(Some(Message { id = 7, author = "bob", content = "whole" }));
	print("-- a key removed, then back");
	global.rooms().at(0).set(None);
	global.rooms().at(0).set(Some(Room { id = 0, name = "again", messages = [] }));
	print("-- the message's payload gone");
	global.messages().at(7).set(None);
	print("-- a whole-root write");
	global.set(Global { rooms = HashMap::new(), messages = HashMap::new(), motd = "reset" });
	print("-- release the client slots, then the bases");
	client_end.send(Frame::Text("{\"Unsubscribe\":[0,[0,1,-2]]}"));
	census("bases only", global);
	global.motd().set("still the root's");
	client_end.send(Frame::Text("{\"Unsubscribe\":[0,[-1]]}"));
	census("released", global);
	global.motd().set("nobody");
	print("done");
}
"##;

#[test]
fn a153_s1_a_mirrored_store_seeds_once_and_patches_at_the_writers_paths() {
    // §3–§5, the server half. The reply is `[channel, base, seed]`, and two
    // replies into one root share the channel (one per root per connection, Q4);
    // the root's seed carries its maps EMPTY (their keys are boundaries, §2.2). A
    // write inside a boundary is a `Set` at its own path (Q6); a write under a
    // key nobody watches sends nothing; one `Subscribe` frame with two slots is
    // answered with ONE patch of their seeds; one turn's writes are ONE patch,
    // the last value per path, and a write above a boundary re-seeds it and
    // covers the writes inside it (Q7); a write of the value held sends nothing;
    // a removed key re-seeds `null`, a through-variant grant whose payload went is
    // `Gone`; a whole-root write re-seeds exactly the slots whose value moved.
    // Releasing every slot and then the grants revokes the channel, and the store
    // keeps no slot and no write log after it. Red on the base: `std::rpc::mirror`
    // does not exist.
    let stdout = run_program("mirror_wire", MIRROR_WIRE);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "before: slots=0 nodes=1 wire=0",
            "reply {\"Success\":[0,-1,{\"rooms\":[],\"messages\":[],\"motd\":\"welcome\"}]}",
            "reply {\"Success\":[0,-2,{\"id\":7,\"author\":\"bob\",\"content\":\"hello\"}]}",
            "granted: slots=2 nodes=4 wire=1",
            "-- a write inside the root's boundary",
            "down {\"Patch\":[0,[{\"Set\":[-1,[2],\"bye\"]}]]}",
            "-- a write inside the message's boundary",
            "down {\"Patch\":[0,[{\"Set\":[-2,[2],\"goodbye\"]}]]}",
            "-- a write under a key nobody watches",
            "-- two slots in one Subscribe",
            "down {\"Patch\":[0,[{\"Seed\":[0,{\"id\":0,\"name\":\"general\",\"messages\":[7]}]},{\"Seed\":[1,{\"id\":8,\"author\":\"amy\",\"content\":\"ho\"}]}]]}",
            "subscribed: slots=4 nodes=7 wire=1",
            "-- one turn, many writes",
            "down {\"Patch\":[0,[{\"Set\":[-1,[2],\"one turn\"]},{\"Set\":[0,[1,1],\"lounge\"]},{\"Set\":[0,[1,2],[7,9]]},{\"Set\":[1,[1,2],\"x\"]},{\"Set\":[1,[1,1],\"zed\"]}]]}",
            "-- a write above a boundary covers the writes inside it",
            "down {\"Patch\":[0,[{\"Seed\":[-2,{\"id\":7,\"author\":\"bob\",\"content\":\"whole\"}]}]]}",
            "-- the same value",
            "-- a key removed, then back",
            "down {\"Patch\":[0,[{\"Seed\":[0,null]}]]}",
            "down {\"Patch\":[0,[{\"Seed\":[0,{\"id\":0,\"name\":\"again\",\"messages\":[]}]}]]}",
            "-- the message's payload gone",
            "down {\"Patch\":[0,[{\"Gone\":-2}]]}",
            "-- a whole-root write",
            "down {\"Patch\":[0,[{\"Seed\":[-1,{\"rooms\":[],\"messages\":[],\"motd\":\"reset\"}]},{\"Seed\":[0,null]},{\"Seed\":[1,null]}]]}",
            "-- release the client slots, then the bases",
            "bases only: slots=1 nodes=1 wire=1",
            "down {\"Patch\":[0,[{\"Set\":[-1,[2],\"still the root's\"]}]]}",
            "released: slots=0 nodes=1 wire=0",
            "done",
        ],
        "the mirrored store's frames went differently:\n{stdout}"
    );
}

/// Grants are the capability (§4.3, Q12): a subscription path is read at the
/// grant's type, and a grant outlives the replies that handed it out.
const MIRROR_GRANTS: &str = r##"import std::hash_map::HashMap;
import std::io::print;
import std::json::json_codec;
import std::option::Option::{ self, None, Some };
import std::reactive::batch;
import std::reactive::store::{ Storable, Store, store_census, store_wire_census };
import std::rpc::{ Dispatcher, DuplexTransport, RpcRequest, drop_session, duplex_pair, encode_request, local_rpc, register_session };
import std::rpc::mirror::{ reply_store, reply_store_some };
import std::wire::{ Frame, Wire };

[derive(Storable, Wire)]
struct Message {
	id: u53,
	content: str,
}

[derive(Storable, Wire)]
enum Presence {
	Offline,
	Online(str),
}

[derive(Storable, Wire)]
struct Global {
	messages: HashMap<u53, Message>,
	presence: Presence,
	motd: str,
}

fun text(frame: Frame): str {
	match frame {
		Frame::Text(let value) => value,
		Frame::Binary(let _bytes) => "<binary>",
	}
}

fun census(label: str, store: Store<Global>) {
	let (slots, nodes) = store_census(store);
	print(i"{label}: slots={slots} nodes={nodes} wire={store_wire_census(store)}");
}

fun main() {
	let codec = json_codec();
	mut messages: HashMap<u53, Message> = HashMap::new();
	messages.insert(7, Message { id = 7, content = "hello" });
	let global = Store::new(Global { messages, presence = Presence::Online("desk"), motd = "hi" });
	let other = Store::new(Global { messages = HashMap::new(), presence = Presence::Offline, motd = "other" });
	let (client_end, server_end) = duplex_pair();
	register_session(7, server_end, codec);
	client_end.on_frame(|frame| print(i"down {text(frame)}"));
	let dispatcher = Dispatcher::new()
		.on("message", |request: RpcRequest| reply_store(request, global.messages().at(7)))
		.on("other", |request: RpcRequest| reply_store(request, other))
		.on("online", |request: RpcRequest| reply_store_some(request, global.presence().online()));
	let local = local_rpc(dispatcher.into_protocol(codec).for_connection(7));
	print(i"reply {text((local.handler)(encode_request(codec, "message", [])))}");
	print(i"reply again {text((local.handler)(encode_request(codec, "message", [])))}");
	print(i"reply other {text((local.handler)(encode_request(codec, "other", [])))}");
	print(i"reply online {text((local.handler)(encode_request(codec, "online", [])))}");
	census("granted", global);
	print("-- a path that names a variant's flag is dropped");
	client_end.send(Frame::Text("{\"Subscribe\":[1,[[3,-1,[1,0]]]]}"));
	print("-- an unknown base is dropped, and the rest of the frame with it");
	client_end.send(Frame::Text("{\"Subscribe\":[0,[[4,-9,[]],[5,-1,[]]]]}"));
	print("-- a malformed step is dropped");
	client_end.send(Frame::Text("{\"Subscribe\":[0,[[6,-1,[\"x\"]]]]}"));
	print("-- a grant can only reach below itself: the message's payload");
	client_end.send(Frame::Text("{\"Subscribe\":[0,[[7,-1,[1]]]]}"));
	census("probed", global);
	let _landed = global.messages().at(7).some().content().patch("edited");
	print("-- the variant ends: the through-variant grant is gone");
	global.presence().set(Presence::Offline);
	print("-- and comes back");
	global.presence().set(Presence::Online("phone"));
	print("-- the deduped grant needs both releases");
	client_end.send(Frame::Text("{\"Unsubscribe\":[0,[7,-1]]}"));
	let _landed = global.messages().at(7).some().content().patch("once");
	client_end.send(Frame::Text("{\"Unsubscribe\":[0,[-1]]}"));
	let _landed = global.messages().at(7).some().content().patch("twice");
	census("one grant left", global);
	print("-- the session ends: every mirror torn down");
	drop_session(7);
	census("dropped", global);
	census("other dropped", other);
	global.presence().set(Presence::Online("gone"));
	print("done");
}
"##;

#[test]
fn a153_s1_a_subscription_reaches_only_below_a_grant_and_a_grant_counts_its_replies() {
    // Q12: the capability is the grant. A path is read at its grant's type, so a
    // step naming a variant's flag, an unknown base, or a malformed key drops the
    // frame (and the rest of it: the steps after it can no longer be read), and
    // nothing can name a path outside the grant. Two replies at one path are ONE
    // grant held twice (A92's dedup by identity), released by two `Unsubscribe`s;
    // a second root is a second channel; a through-variant grant goes `Gone` and
    // is re-seeded when its variant returns; and the session's end tears every
    // mirror down — no slot, no node, no write log left on either store.
    let stdout = run_program("mirror_grants", MIRROR_GRANTS);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "reply {\"Success\":[0,-1,{\"id\":7,\"content\":\"hello\"}]}",
            "reply again {\"Success\":[0,-1,{\"id\":7,\"content\":\"hello\"}]}",
            "reply other {\"Success\":[1,-1,{\"messages\":[],\"presence\":\"Offline\",\"motd\":\"other\"}]}",
            "reply online {\"Success\":[0,-2,\"desk\"]}",
            "granted: slots=2 nodes=5 wire=1",
            "-- a path that names a variant's flag is dropped",
            "-- an unknown base is dropped, and the rest of the frame with it",
            "-- a malformed step is dropped",
            "-- a grant can only reach below itself: the message's payload",
            "down {\"Patch\":[0,[{\"Seed\":[7,{\"id\":7,\"content\":\"hello\"}]}]]}",
            "probed: slots=3 nodes=6 wire=1",
            "down {\"Patch\":[0,[{\"Set\":[-1,[1,1],\"edited\"]},{\"Set\":[7,[1],\"edited\"]}]]}",
            "-- the variant ends: the through-variant grant is gone",
            "down {\"Patch\":[0,[{\"Gone\":-2}]]}",
            "-- and comes back",
            "down {\"Patch\":[0,[{\"Seed\":[-2,\"phone\"]}]]}",
            "-- the deduped grant needs both releases",
            "down {\"Patch\":[0,[{\"Set\":[-1,[1,1],\"once\"]}]]}",
            "one grant left: slots=1 nodes=3 wire=1",
            "-- the session ends: every mirror torn down",
            "dropped: slots=0 nodes=1 wire=0",
            "other dropped: slots=0 nodes=1 wire=0",
            "done",
        ],
        "the grants went differently:\n{stdout}"
    );
}

/// A turn that writes more than the root's write log keeps.
const MIRROR_LAG: &str = r##"import std::io::print;
import std::json::json_codec;
import std::reactive::batch;
import std::reactive::store::{ Storable, Store };
import std::rpc::{ Dispatcher, DuplexTransport, RpcRequest, duplex_pair, encode_request, local_rpc, register_session };
import std::rpc::mirror::reply_store;
import std::wire::{ Frame, Wire };

[derive(Storable, Wire)]
struct Board {
	motd: str,
	count: i32,
}

fun text(frame: Frame): str {
	match frame {
		Frame::Text(let value) => value,
		Frame::Binary(let _bytes) => "<binary>",
	}
}

fun main() {
	let codec = json_codec();
	let board = Store::new(Board { motd = "hi", count = 0 });
	let (client_end, server_end) = duplex_pair();
	register_session(3, server_end, codec);
	client_end.on_frame(|frame| print(i"down {text(frame)}"));
	let dispatcher = Dispatcher::new().on("board", |request: RpcRequest| reply_store(request, board));
	let local = local_rpc(dispatcher.into_protocol(codec).for_connection(3));
	print(i"reply {text((local.handler)(encode_request(codec, "board", [])))}");
	print("-- more writes in one turn than the log keeps");
	batch(|| {
		mut n = 0;
		for n < 1100 {
			board.count().set(n);
			n += 1;
		}
	});
	print("-- the next turn is told by path again");
	board.motd().set("caught up");
	print("done");
}
"##;

#[test]
fn a153_s1_a_log_that_outran_its_ceiling_reseeds_the_slot() {
    // The root's write log is a `DeltaLog`: past its ceiling it drops its
    // history, and a forward whose cursor fell behind cannot say what moved — so
    // it re-seeds the slot whole, once, and the next turn is told by path again.
    let stdout = run_program("mirror_lag", MIRROR_LAG);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "reply {\"Success\":[0,-1,{\"motd\":\"hi\",\"count\":0}]}",
            "-- more writes in one turn than the log keeps",
            "down {\"Patch\":[0,[{\"Seed\":[-1,{\"motd\":\"hi\",\"count\":1099}]}]]}",
            "-- the next turn is told by path again",
            "down {\"Patch\":[0,[{\"Set\":[-1,[0],\"caught up\"]}]]}",
            "done",
        ],
        "the lagging forward went differently:\n{stdout}"
    );
}

// --- A153 S2: the mirrored store's client half (`mirrored-store.md` §6) -------

/// A store mirror minted UNLEASED and wired by hand (`mint_store` over a local
/// transport: what the generated stub writes), the frames traced both ways.
const MIRROR_CLIENT: &str = r##"import std::hash_map::HashMap;
import std::io::print;
import std::json::json_codec;
import std::option::Option::{ self, None, Some };
import std::reactive::{ Owner, Source, run_with_owner };
import std::reactive::store::{ RemoteStoreSome, Storable, Store };
import std::rpc::{ Dispatcher, DuplexTransport, ReactiveClient, RpcRequest, call_reading, duplex_pair, local_rpc, register_session };
import std::time::{ Duration, sleep_for };
import std::rpc::mirror::{ mint_store, read_store_reply, reply_store };
import std::wire::{ Deserializer, Frame, Wire };

[derive(Storable, Wire)]
struct Message {
	id: u53,
	content: str,
}

[derive(Storable, Wire)]
enum Presence {
	Offline,
	Online(str),
}

[derive(Storable, Wire)]
struct Global {
	messages: HashMap<u53, Message>,
	presence: Presence,
	motd: str,
}

fun text(frame: Frame): str {
	match frame {
		Frame::Text(let value) => value,
		Frame::Binary(let _bytes) => "<binary>",
	}
}

fun main() {
	let codec = json_codec();
	mut messages: HashMap<u53, Message> = HashMap::new();
	messages.insert(7, Message { id = 7, content = "hello" });
	let global = Store::new(Global { messages, presence = Presence::Online("desk"), motd = "welcome" });
	let (client_end, client_relay) = duplex_pair();
	let (server_end, server_relay) = duplex_pair();
	client_relay.on_frame(|frame| {
		print(i"  up   {text(frame)}");
		server_relay.send(frame);
	});
	server_relay.on_frame(|frame| {
		print(i"  down {text(frame)}");
		client_relay.send(frame);
	});
	register_session(7, server_end, codec);
	let client = ReactiveClient::new(client_end, codec);
	let dispatcher = Dispatcher::new().on("global", |request: RpcRequest| {
		print("  rpc  global");
		reply_store(request, global)
	});
	let local = local_rpc(dispatcher.into_protocol(codec).for_connection(7));
	let g: RemoteStoreSome<Global> = mint_store(client, || call_reading(local, codec, "global", [], |reply: Deserializer| read_store_reply<Global>(reply, false)));
	print(i"unleased: {g.get().is_some()}");
	let page = Owner::new();
	run_with_owner(page, || {
		g.motd().effect(|motd| print(i"motd {motd.unwrap_or("-")}"));
		g.presence().online().effect(|place| print(i"online {place.unwrap_or("-")}"));
		g.presence().is_online().effect(|on| print(i"is online {on}"));
		g.messages().at(7).some().content().effect(|content| print(i"seven {content.unwrap_or("-")}"));
		g.messages().at(7).some().id().effect(|id| print(i"seven's id {id.unwrap_or(0)}"));
	});
	sleep_for(Duration::millis(0));
	print("-- server writes");
	global.motd().set("bye");
	global.presence().set(Presence::Offline);
	let _edited = global.messages().at(7).some().content().patch("goodbye");
	print("-- the page goes: the grant is let go");
	page.dispose();
	sleep_for(Duration::millis(0));
	global.motd().set("unwatched");
	print(i"held: {g.get().map(|value: Global| value.motd).unwrap_or("-")}");
	print(i"the released key left the replica: {g.messages().at(7).get().flatten().is_none()}");
	print("-- a new lease mints again");
	let again = Owner::new();
	run_with_owner(again, || {
		g.motd().effect(|motd| print(i"again {motd.unwrap_or("-")}"));
	});
	sleep_for(Duration::millis(0));
	again.dispose();
	sleep_for(Duration::millis(0));
	print("done");
}
"##;

#[test]
fn a153_s2_a_store_mirror_mints_on_its_first_hold_and_patches_its_replica() {
    // §6. Unleased, the mirror asked nothing (`unleased: false` holds no value);
    // the first hold issues the call, and the reply's seed lands in the replica
    // as a comparing write (the root is a maybe until then, Q10). A projection
    // reads the replica; a hold under a map key puts that key's slot on the wire
    // with the turn's ONE `Subscribe` — two handles under key 7 share one slot —
    // and its seed and `Set`s land through the key's own handle. A variant's flag
    // and its through-variant handle follow the server's switch. The page's
    // disposal releases the key's slot (and the key leaves the replica: memory
    // follows demand) and then the grant's base; the replica keeps its last value
    // (`held: bye`), and a new hold mints again — on a fresh channel, since the
    // server revoked the old one with its last grant — and the re-seed is a
    // comparing write (`again bye`, then the write it missed).
    let stdout = run_program("mirror_client", MIRROR_CLIENT);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "unleased: false",
            "rpc  global",
            "motd -",
            "online -",
            "is online false",
            "seven -",
            "seven's id 0",
            "motd welcome",
            "online desk",
            "is online true",
            "seven -",
            "seven's id 0",
            "up   {\"Subscribe\":[0,[[0,-1,[0,7]]]]}",
            "down {\"Patch\":[0,[{\"Seed\":[0,{\"id\":7,\"content\":\"hello\"}]}]]}",
            "seven hello",
            "seven's id 7",
            "-- server writes",
            "down {\"Patch\":[0,[{\"Set\":[-1,[2],\"bye\"]}]]}",
            "motd bye",
            "down {\"Patch\":[0,[{\"Set\":[-1,[1],\"Offline\"]}]]}",
            "online -",
            "is online false",
            "down {\"Patch\":[0,[{\"Set\":[0,[1,1],\"goodbye\"]}]]}",
            "seven goodbye",
            "-- the page goes: the grant is let go",
            "up   {\"Unsubscribe\":[0,[0]]}",
            "up   {\"Unsubscribe\":[0,[-1]]}",
            "held: bye",
            "the released key left the replica: true",
            "-- a new lease mints again",
            "rpc  global",
            "again bye",
            "again unwatched",
            "up   {\"Unsubscribe\":[1,[-1]]}",
            "done",
        ],
        "the store mirror went differently:\n{stdout}"
    );
}

/// A `[service]` whose `[rpc]`s return a `Store<T>` and a `StoreSome<P>`, reached
/// through its GENERATED client — the stub, the origin table and the replier the
/// expansion writes.
const MIRROR_STUB: &str = r##"import std::hash_map::HashMap;
import std::io::print;
import std::json::json_codec;
import std::option::Option::{ self, None, Some };
import std::reactive::{ Owner, Source, run_with_owner };
import std::reactive::store::{ RemoteStoreSome, Storable, Store, StoreSome };
import std::rpc::{ DuplexEnd, DuplexTransport, ReactiveClient, duplex_pair, local_rpc, register_session };
import std::time::{ Duration, sleep_for };
import std::wire::{ Frame, Wire };

[derive(Storable, Wire)]
struct Message {
	id: u53,
	content: str,
}

[derive(Storable, Wire)]
struct Global {
	messages: HashMap<u53, Message>,
	motd: str,
}

fun seeded(): Global {
	mut messages: HashMap<u53, Message> = HashMap::new();
	messages.insert(7, Message { id = 7, content = "hello" });
	Global { messages, motd = "welcome" }
}

let shared: Store<Global> = Store::new(seeded());

[service(BoardClient)]
struct Board {}

impl Board {
	[rpc]
	fun global(self): Store<Global> {
		shared
	}

	[rpc]
	fun message(self, id: u53): StoreSome<Message> {
		shared.messages().at(id).some()
	}

	[rpc]
	fun edit(self, id: u53, content: str): bool {
		shared.messages().at(id).some().content().patch(content)
	}
}

fun text(frame: Frame): str {
	match frame {
		Frame::Text(let value) => value,
		Frame::Binary(let _bytes) => "<binary>",
	}
}

fun traced_pair(): (DuplexEnd, DuplexEnd) {
	let (client_end, client_relay) = duplex_pair();
	let (server_end, server_relay) = duplex_pair();
	client_relay.on_frame(|frame| {
		print(i"  up   {text(frame)}");
		server_relay.send(frame);
	});
	server_relay.on_frame(|frame| {
		print(i"  down {text(frame)}");
		client_relay.send(frame);
	});
	(client_end, server_end)
}

fun main() {
	let (client_end, server_end) = traced_pair();
	register_session(7, server_end, json_codec());
	let transport = local_rpc(Board {}.dispatcher().into_protocol(json_codec()).for_connection(7));
	let client = BoardClient { transport, codec = json_codec(), reactive = ReactiveClient::new(client_end, json_codec()) };
	let g: RemoteStoreSome<Global> = client.global();
	let m: RemoteStoreSome<Message> = client.message(7);
	print(i"one origin, one mirror: {client.global().identity() == g.identity()}");
	let page = Owner::new();
	run_with_owner(page, || {
		g.motd().effect(|motd| print(i"motd {motd.unwrap_or("-")}"));
		m.content().effect(|content| print(i"message {content.unwrap_or("-")}"));
	});
	sleep_for(Duration::millis(0));
	print(i"edit: {client.edit(7, "goodbye").unwrap_or(false)}");
	shared.motd().set("bye");
	page.dispose();
	sleep_for(Duration::millis(0));
	print("done");
}
"##;

#[test]
fn a153_s2_a_generated_store_stub_mints_on_its_first_hold_and_lets_go_with_its_last() {
    // §3.1 + §6 through the expansion: `client.global()` and `client.message(7)`
    // are SYNC stubs answering `RemoteStoreSome<..>` (Q10), deduped per origin
    // (A134: two `global()` calls are one mirror). The first holds issue the
    // calls; both grants land on ONE channel (one per root, Q4), bases -1 and
    // -2; an `[rpc]` write inside the message's boundary is a `Set` on its base,
    // a server write to the root's field a `Set` on the root's; the page's
    // disposal lets both grants go. Verified green on store-49's tree with the
    // patch applied, on both backends.
    let stdout = run_program("mirror_stub", MIRROR_STUB);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "one origin, one mirror: true",
            "motd -",
            "message -",
            "motd welcome",
            "message hello",
            "down {\"Patch\":[0,[{\"Set\":[-2,[1],\"goodbye\"]}]]}",
            "message goodbye",
            "edit: true",
            "down {\"Patch\":[0,[{\"Set\":[-1,[1],\"bye\"]}]]}",
            "motd bye",
            "up   {\"Unsubscribe\":[0,[-1]]}",
            "up   {\"Unsubscribe\":[0,[-2]]}",
            "done",
        ],
        "the generated store stub went differently:\n{stdout}"
    );
}

/// A remote handle's `states()` (A150's leased pipe, `mirrored-store.md` §6.1),
/// the minting call made to FAIL once; down frames land a microtask later, each in
/// its own turn, as a socket's do.
const MIRROR_STATES: &str = r##"import std::hash_map::HashMap;
import std::io::print;
import std::json::json_codec;
import std::option::Option::{ self, None, Some };
import std::reactive::store::{ RemoteStoreSome, Storable, Store };
import std::reactive::transient::{ TransientSource, TransientState };
import std::reactive::{ Owner, Source, batch, combine, queue_microtask, run_with_owner };
import std::result::Result::{ self, Err };
import std::rpc::mirror::{ StoreReply, mint_store, read_store_reply, reply_store };
import std::rpc::{
	Dispatcher,
	DuplexTransport,
	ReactiveClient,
	RpcError,
	RpcRequest,
	call_reading,
	duplex_pair,
	local_rpc,
	register_session,
};
import std::shared::Shared;
import std::time::{ Duration, sleep_for };
import std::wire::{ Deserializer, Frame, Wire };

[derive(Storable, Wire)]
struct Message {
	id: u53,
	content: str,
}

[derive(Storable, Wire)]
struct Global {
	messages: HashMap<u53, Message>,
	motd: str,
}

fun text(frame: Frame): str {
	match frame {
		Frame::Text(let value) => value,
		Frame::Binary(let _bytes) => "<binary>",
	}
}

fun shown<T>(state: TransientState<T, RpcError>, show: |T| str): str {
	match state {
		TransientState::Pending => "Pending",
		TransientState::Ready(let value) => i"Ready({show(value)})",
		TransientState::Refreshing(let value) => i"Refreshing({show(value)})",
		TransientState::Failed(let error, let stale) => i"Failed({error.debug()}, {stale.map(show).unwrap_or("-")})",
		TransientState::Absent => "Absent",
	}
}

fun main() {
	let codec = json_codec();
	mut messages: HashMap<u53, Message> = HashMap::new();
	messages.insert(7, Message { id = 7, content = "hello" });
	let global = Store::new(Global { messages, motd = "welcome" });
	let (client_end, client_relay) = duplex_pair();
	let (server_end, server_relay) = duplex_pair();
	client_relay.on_frame(|frame| {
		print(i"  up   {text(frame)}");
		server_relay.send(frame);
	});
	// Down frames land a microtask later, each in its own turn, as a socket's do.
	server_relay.on_frame(|frame| {
		print(i"  down {text(frame)}");
		queue_microtask(|| client_relay.send(frame));
	});
	register_session(7, server_end, codec);
	let client = ReactiveClient::new(client_end, codec);
	let fail_next = Shared::new(true);
	let dispatcher = Dispatcher::new().on("global", |request: RpcRequest| {
		print("  rpc  global");
		reply_store(request, global)
	});
	let local = local_rpc(dispatcher.into_protocol(codec).for_connection(7));
	let g: RemoteStoreSome<Global> = mint_store(client, || {
		if fail_next.read() {
			fail_next.write() = false;
			let refused: Result<StoreReply<Global>, RpcError> = Err(RpcError::Unavailable);
			ret refused;
		}
		call_reading(local, codec, "global", [], |reply: Deserializer| read_store_reply<Global>(reply, false))
	});
	print(i"passive: {shown(g.state().get(), |value: Global| value.motd)}");
	let page = Owner::new();
	run_with_owner(page, || {
		g.states().effect(|state| print(i"root {shown(state, |value: Global| value.motd)}"));
	});
	sleep_for(Duration::millis(0));
	print("-- the next hold asks again");
	page.dispose();
	sleep_for(Duration::millis(0));
	let again = Owner::new();
	run_with_owner(again, || {
		g.states().effect(|state| print(i"root {shown(state, |value: Global| value.motd)}"));
		g
			.messages()
			.at(7)
			.some()
			.states()
			.effect(|state| print(i"seven {shown(state, |value: Message| value.content)}"));
		g
			.messages()
			.at(9)
			.some()
			.states()
			.effect(|state| print(i"nine {shown(state, |value: Message| value.content)}"));
		g.messages().at(7).some().is_pending().effect(|pending| print(i"seven pending {pending}"));
		let content: dyn Source<Option<str>> = g.messages().at(7).some().content();
		let id: dyn Source<Option<u53>> = g.messages().at(7).some().id();
		combine((content, id)).effect(|both| {
			let (text, number) = both;
			print(i"seven's pair {number.unwrap_or(0)}:{text.unwrap_or("-")}");
		});
	});
	sleep_for(Duration::millis(0));
	print("-- two fields in one patch: one turn on the client");
	batch(|| {
		let _content = global.messages().at(7).some().content().patch("both");
		let _id = global.messages().at(7).some().id().patch(70);
	});
	sleep_for(Duration::millis(0));
	print("-- the key goes, then comes back");
	global.messages().at(7).set(None);
	sleep_for(Duration::millis(0));
	global.messages().at(7).set(Some(Message { id = 7, content = "back" }));
	sleep_for(Duration::millis(0));
	again.dispose();
	sleep_for(Duration::millis(0));
	print("done");
}
"##;

#[test]
fn a153_s2_a_remote_handle_tells_pending_ready_absent_and_failed_through_states() {
    // §6.1. `state()` reports without holding (`passive: Pending`). The first
    // hold's call fails: `Failed(Unavailable, -)`, no value to carry. The next
    // hold asks again and reads `Pending` while it is out, `Ready` with the
    // reply; a key's handle reads `Pending` until ITS seed lands — `Ready` for a
    // key the server holds, `Absent` for one it does not (both seeded in the
    // turn's one `Subscribe`/`Patch`) — and `is_pending` follows. Two fields
    // written in one server turn land in ONE client turn: the pair's observer
    // wakes once, never with the content beside the old id. A removed key reads
    // `Absent`, and `Ready` again when it is back.
    let stdout = run_program("mirror_states", MIRROR_STATES);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "passive: Pending",
            "root Pending",
            "root Failed(RpcError::Unavailable, -)",
            "-- the next hold asks again",
            "rpc  global",
            "root Pending",
            "seven Pending",
            "nine Pending",
            "seven pending true",
            "seven's pair 0:-",
            "root Ready(welcome)",
            "seven Pending",
            "nine Pending",
            "seven pending true",
            "seven's pair 0:-",
            "up   {\"Subscribe\":[0,[[0,-1,[0,7]],[1,-1,[0,9]]]]}",
            "down {\"Patch\":[0,[{\"Seed\":[0,{\"id\":7,\"content\":\"hello\"}]},{\"Seed\":[1,null]}]]}",
            "root Ready(welcome)",
            "seven Ready(hello)",
            "nine Absent",
            "seven pending false",
            "seven's pair 7:hello",
            "-- two fields in one patch: one turn on the client",
            "down {\"Patch\":[0,[{\"Set\":[0,[1,1],\"both\"]},{\"Set\":[0,[1,0],70]}]]}",
            "root Ready(welcome)",
            "seven Ready(both)",
            "seven pending false",
            "seven's pair 70:both",
            "-- the key goes, then comes back",
            "down {\"Patch\":[0,[{\"Seed\":[0,null]}]]}",
            "root Ready(welcome)",
            "seven Absent",
            "seven pending false",
            "seven's pair 0:-",
            "down {\"Patch\":[0,[{\"Seed\":[0,{\"id\":7,\"content\":\"back\"}]}]]}",
            "root Ready(welcome)",
            "seven Ready(back)",
            "seven pending false",
            "seven's pair 7:back",
            "up   {\"Unsubscribe\":[0,[1]]}",
            "up   {\"Unsubscribe\":[0,[0]]}",
            "up   {\"Unsubscribe\":[0,[-1]]}",
            "done",
        ],
        "the remote handle's states went differently:\n{stdout}"
    );
}

// --- A153 S4: reconnect (`mirrored-store.md` §7) ------------------------------

/// A mirrored store over a connection that DROPS and is re-made: the old session
/// goes with its channels, a fresh one takes the same wire, and the client's
/// replay (what `reattach_mirrors` runs) rebinds the mirror. Down frames land a
/// microtask later, each in its own turn.
const MIRROR_RECONNECT: &str = r##"import std::hash_map::HashMap;
import std::io::print;
import std::json::json_codec;
import std::option::Option::{ self, None, Some };
import std::reactive::store::{ RemoteStoreSome, Storable, Store };
import std::reactive::transient::{ TransientSource, TransientState };
import std::reactive::{ Owner, Source, queue_microtask, run_with_owner };
import std::rpc::mirror::{ mint_store, read_store_reply, reply_store };
import std::rpc::{
	Dispatcher,
	DuplexTransport,
	ReactiveClient,
	RpcError,
	RpcRequest,
	call_reading,
	drop_session,
	duplex_pair,
	local_rpc,
	register_session,
};
import std::time::{ Duration, sleep_for };
import std::wire::{ Deserializer, Frame, Wire };

[derive(Storable, Wire)]
struct Message {
	id: u53,
	content: str,
}

[derive(Storable, Wire)]
struct Global {
	messages: HashMap<u53, Message>,
	motd: str,
}

fun text(frame: Frame): str {
	match frame {
		Frame::Text(let value) => value,
		Frame::Binary(let _bytes) => "<binary>",
	}
}

fun shown<T>(state: TransientState<T, RpcError>, show: |T| str): str {
	match state {
		TransientState::Pending => "Pending",
		TransientState::Ready(let value) => i"Ready({show(value)})",
		TransientState::Refreshing(let value) => i"Refreshing({show(value)})",
		TransientState::Failed(let error, let _stale) => i"Failed({error.debug()})",
		TransientState::Absent => "Absent",
	}
}

fun main() {
	let codec = json_codec();
	mut messages: HashMap<u53, Message> = HashMap::new();
	messages.insert(7, Message { id = 7, content = "hello" });
	messages.insert(8, Message { id = 8, content = "hi" });
	let global = Store::new(Global { messages, motd = "welcome" });
	let (client_end, client_relay) = duplex_pair();
	let (server_end, server_relay) = duplex_pair();
	client_relay.on_frame(|frame| {
		print(i"  up   {text(frame)}");
		server_relay.send(frame);
	});
	// Down frames land a microtask later, each in its own turn, as a socket's do.
	server_relay.on_frame(|frame| {
		print(i"  down {text(frame)}");
		queue_microtask(|| client_relay.send(frame));
	});
	register_session(7, server_end, codec);
	let client = ReactiveClient::new(client_end, codec);
	let dispatcher = Dispatcher::new().on("global", |request: RpcRequest| {
		print("  rpc  global");
		reply_store(request, global)
	});
	let local = local_rpc(dispatcher.into_protocol(codec).for_connection(7));
	let g: RemoteStoreSome<Global> = mint_store(client, || call_reading(local, codec, "global", [], |reply: Deserializer| read_store_reply<Global>(reply, false)));
	let page = Owner::new();
	run_with_owner(page, || {
		g.motd().effect(|motd| print(i"motd {motd.unwrap_or("-")}"));
		g
			.messages()
			.at(7)
			.some()
			.content()
			.effect(|content| print(i"seven {content.unwrap_or("-")}"));
		g
			.messages()
			.at(8)
			.some()
			.content()
			.effect(|content| print(i"eight {content.unwrap_or("-")}"));
		g.states().effect(|state| print(i"root {shown(state, |value: Global| value.motd)}"));
		g
			.messages()
			.at(7)
			.some()
			.states()
			.effect(|state| print(i"seven's state {shown(state, |value: Message| value.content)}"));
	});
	sleep_for(Duration::millis(0));
	print("-- the connection drops: the old session goes with its channels");
	client.connection_lost();
	drop_session(7);
	sleep_for(Duration::millis(0));
	print("-- written while down: the motd and seven; eight untouched");
	global.motd().set("while down");
	let _edited = global.messages().at(7).some().content().patch("edited");
	print("-- a hold taken while down waits for the replay");
	let late = Owner::new();
	run_with_owner(late, || {
		g.messages().at(8).some().id().effect(|id| print(i"eight's id {id.unwrap_or(0)}"));
	});
	sleep_for(Duration::millis(0));
	print("-- a fresh session on the wire, and the replay");
	register_session(7, server_end, codec);
	client.replay_dynamic();
	sleep_for(Duration::millis(0));
	sleep_for(Duration::millis(0));
	print("-- the fresh channel follows");
	global.motd().set("after");
	sleep_for(Duration::millis(0));
	late.dispose();
	page.dispose();
	sleep_for(Duration::millis(0));
	print("done");
}
"##;

#[test]
fn a153_s4_a_reconnect_replays_the_root_once_resubscribes_in_one_frame_and_reseeds_by_comparison() {
    // §7. While down every handle keeps its last value and reads `Refreshing`
    // (the drop is what `dispose_on_close` tells the client), and a hold taken
    // meanwhile asks nothing. The replay re-issues the root's call ONCE; the
    // seed lands over the replica, keeping the keys it holds (a seed carries
    // its maps empty), so only the field written while down wakes (`motd while
    // down`); every live boundary is re-subscribed in ONE `Subscribe` on the
    // fresh channel, and the re-seeds compare: seven's edit wakes seven, eight
    // — untouched — wakes nothing, nor does the hold taken while down. The
    // fresh channel follows, and the page's release lets it go.
    let stdout = run_program("mirror_reconnect", MIRROR_RECONNECT);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "rpc  global",
            "motd -",
            "seven -",
            "eight -",
            "root Pending",
            "seven's state Pending",
            "motd welcome",
            "seven -",
            "eight -",
            "root Ready(welcome)",
            "seven's state Pending",
            "up   {\"Subscribe\":[0,[[0,-1,[0,7]],[1,-1,[0,8]]]]}",
            "down {\"Patch\":[0,[{\"Seed\":[0,{\"id\":7,\"content\":\"hello\"}]},{\"Seed\":[1,{\"id\":8,\"content\":\"hi\"}]}]]}",
            "seven hello",
            "eight hi",
            "root Ready(welcome)",
            "seven's state Ready(hello)",
            "-- the connection drops: the old session goes with its channels",
            "root Refreshing(welcome)",
            "seven's state Refreshing(hello)",
            "-- written while down: the motd and seven; eight untouched",
            "-- a hold taken while down waits for the replay",
            "eight's id 8",
            "-- a fresh session on the wire, and the replay",
            "root Refreshing(welcome)",
            "seven's state Refreshing(hello)",
            "rpc  global",
            "motd while down",
            "root Ready(while down)",
            "up   {\"Subscribe\":[1,[[0,-1,[0,7]],[1,-1,[0,8]]]]}",
            "down {\"Patch\":[1,[{\"Seed\":[0,{\"id\":7,\"content\":\"edited\"}]},{\"Seed\":[1,{\"id\":8,\"content\":\"hi\"}]}]]}",
            "seven edited",
            "root Ready(while down)",
            "seven's state Ready(edited)",
            "-- the fresh channel follows",
            "down {\"Patch\":[1,[{\"Set\":[-1,[1],\"after\"]}]]}",
            "motd after",
            "root Ready(after)",
            "root Ready(after)",
            "up   {\"Unsubscribe\":[1,[1]]}",
            "up   {\"Unsubscribe\":[1,[0]]}",
            "up   {\"Unsubscribe\":[1,[-1]]}",
            "done",
        ],
        "the mirrored store's reconnect went differently:\n{stdout}"
    );
}
