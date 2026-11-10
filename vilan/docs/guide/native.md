# Native binaries

Vilan has a second backend. `--backend rust` emits your program as Rust source,
writes it into a cargo project under `dist/native/`, and builds it with the
`cargo` already on your machine:

```sh
vilan build --backend rust hello.vl   # writes dist/native/hello/ and builds it
vilan run   --backend rust hello.vl   # builds, then runs the binary
```

The program is the same program. The point of the backend is that its output is
a binary with no runtime host — no Node, no browser, no garbage collector — and
the way that is kept honest is a differential: every program the backend accepts
must print **byte-identical** output under both backends, and a gate in the
compiler's own suite proves it over the test corpus.

## What it reaches, and what it refuses by name

What the backend reaches today:

- structs, enums, `Option` and `Result`, generics (monomorphised per type
  argument), traits, trait objects (`dyn`), overloaded operators
- `str`, `List`, `HashMap`, `HashSet`, closures, `impl` blocks, `match`, loops,
  recursion, module-level bindings, `lazy`
- `async`, `await`, `Task` and `Nursery`, on a single-threaded executor of the
  runtime's own
- `std::json`, `std::db` (SQLite, through a separate runtime crate that a
  program links only when it reaches `std::db`), `std::bytes`, `std::binary`,
  `std::time`'s clock and `Instant::to_iso`, `str::index_of` and
  `last_index_of` (in UTF-16 code units, as on node), and `std::fs`'s reads,
  writes and directories — `create_dir_all`, `remove_dir_all`, `copy_dir`,
  `scan_dir` and `read_dir_all` with node's results and failures (a listing
  comes back sorted by name, which node does not promise either)
- `std::crypto`: SHA-256, SHA-384 and SHA-512, HMAC-SHA-512, PBKDF2,
  `random_bytes` and `random_uuid` from the operating system's random source —
  and node:crypto's `pbkdf2Sync` when a program binds it itself, with the
  `Buffer` it answers read back through `toString("hex")`. Everything past
  SHA-256 lives in a second runtime crate a program links only when it reaches
  one of them, since OS randomness is the one thing here that needs a
  dependency
- **servers**: `std::http` — `Server::builder()` with `serve_build`,
  `cache_build` and its conditional GET, `on_request`, `on_start` — and
  `std::rpc::server`: a `[service]` mounted with `Service::new` or
  `Service::factory`, the WebSocket upgrade with its handshake gate
  (`authorize`), and `[expose]`/`[expose(keyed)]` mirrors a vilan client
  subscribes to, byte-for-byte on the wire with the node build
- **rpc clients**: `std::rpc`'s WebSocket client over `ws://` — a generated
  client's `connect` (and `connect_with` a credential), `connect_socket`,
  calls, mirrors, the reconnect loop with its backoff, and a dial that honours
  its nursery's cancel (`dial_within`'s deadline included). A native client
  talks to a native server or a node one, and a node client to either

What it does **not** reach yet — each refused by name, with the construct in the
message, rather than silently mis-compiled: the browser platform (`std::web::dom`,
`std::web::ui`, `std::web::prelude`, `std::web::router`), a `resource` type with a `Drop` impl
(deterministic teardown natively is its own slice; a resource with no `Drop`,
such as a pipe node, builds as an ordinary move-only type), a handful of host bindings
(`std::fs`'s file handles and its watcher among them),
`--watch` (a native round is a full `cargo build`; the dev loop's hot-swap
belongs to the JS backend), and **TLS**: a native client dialling `wss://` stops
at the dial with a message saying the native backend has no TLS — the URL is a
value, so this one is refused when the program runs rather than when it builds.
Dial `ws://`, behind a TLS-terminating proxy (caddy, nginx) when the server
needs one, as a native server is deployed.

If your program uses one of these, `--backend rust` tells you which and stops.
Build it with `--backend js`, which is still the default.

**`Bytes` is shared.** A `Bytes` is one buffer behind every holder, exactly as
the `Uint8Array` it is on the JS backend: `let b = a` is an alias, and a
`set`, `fill` or `copy_into` through either is seen through both. `slice` and
`Bytes::concat` are the two that make a new buffer.

**`std::db` binds and reads the same values.** A parameter list is a
`List<dyn SqlValue>` on both backends, so what may be bound — a number, a
`str`, a `bool`, `Bytes` as a BLOB, an `Option` of one — is the checker's
answer, not a run-time refusal on node and a rustc error here. A BLOB reads
back through `Row::bytes` as the bytes that were bound. A typed reader
(`text`, `integer`, …) meets a NULL with the same panic on both backends,
naming the column; the `optional_` readers are the nullable door. One
difference is node's, not the program's: node binds every JS number as a
REAL, so a column with no declared type (no affinity) stores `7` as `7.0`
there and as INTEGER natively. Declare the column `INTEGER` (or `NUMERIC`)
and both read back the same.

**A server answers node's bytes, less node's `Date`.** The native HTTP server
keeps connections alive as node does — HTTP/1.1 persistent connections,
`Connection: close` honoured from either side, HTTP/1.0 answered and closed,
pipelined requests answered in order — and writes node's head field for field:
node's reason phrases, `Connection: keep-alive` with `Keep-Alive: timeout=5`, a
streamed response `Transfer-Encoding: chunked`. The one field it does not write
is node's `Date`. Node's three deadlines hold natively too, with node's
defaults: a request's head within 60 s, the whole request within 300 s (each
answered `408 Request Timeout` when missed), and 5 s idle between requests;
`ServerBuilder`'s `headers_timeout`, `request_timeout` and `keep_alive_timeout`
set them on both backends. HTTP/2 and TLS are not here: a native server is
deployed behind a proxy that terminates TLS.

**A server with no `host` binds dual-stack, as node's does.** Node's
`listen(port)` with no host takes `::` with IPv6-only switched off, so one
socket answers IPv4 and IPv6 clients on every interface, and the native server
binds the same socket the same way — on Windows too, where an IPv6 socket is
IPv6-only unless told otherwise. On a machine without IPv6 both take `0.0.0.0`
instead. `Request::peer_address` therefore spells an IPv4 client of a server
with no host mapped on both backends, `::ffff:127.0.0.1`, and an IPv6 one as
itself. Name the bind with `ServerBuilder::host` to narrow it:
`host("127.0.0.1")` and `host("0.0.0.0")` are IPv4 only and read `127.0.0.1`,
`host("::1")` is IPv6 loopback only and reads `::1`, and `host("::")` is the
same dual-stack socket as no host at all.

**A native server stops gracefully on SIGTERM.** `Server::stop()` works as it
does on node. Beyond it, a program that starts a server answers SIGTERM, SIGINT
and SIGHUP (Ctrl-C, and the console's close on Windows) by stopping its
servers: it stops accepting, closes the connections that are only waiting and
the upgraded sockets, lets a request already being answered finish — and then
the program ends, where node would have died of the signal. It ends whatever
else is still pending: a timer, a detached `async { for { ..; sleep(..) } }`
task, a `main` still suspended (which ends without a failure) — node abandons
all of them when it dies, so a native program that waited for them would be one
SIGTERM no longer stops. A second signal ends the process at once, for a server
whose open response never ends. The handler
lives in a third runtime crate, linked only by a program that serves, so a
native server build fetches the `ctrlc` crate the first time.

**A native client's socket is the host's, with two differences you can see
only from outside the program.** The address lookup and the TCP connect run on
a helper thread (Rust's standard library has no non-blocking connect), so a
slow DNS answer does not stall the event loop; and no compression extension is
offered on the handshake. Events arrive one per turn of the event loop, as the
host's do, so the program sees the same order of messages, errors and closes.

## Debug by default

`vilan run --backend rust` builds in **debug**, and so does `vilan build
--backend rust`. rustc is the inner loop from here on: a release build of a
seven-hundred-line program took over a second of CPU in the design probe, and a
real program is orders larger. When you want the optimised binary, build the
generated project yourself:

```sh
cargo build --release --manifest-path dist/native/hello/Cargo.toml
```

One exception is made for you: a program that reaches PBKDF2 gets the crypto
runtime crate optimised even in debug, because a password hash's cost is its
iteration count by design — 100,000 rounds of HMAC-SHA-512 takes seconds
unoptimised and a tenth of one optimised. Your own code keeps the debug build.

## Numbers

`i53` and `u53` are JavaScript-shaped types — the exact-integer range of a
double. Natively they are `i64` and `u64`, which is wider, but **the guarantee
is still the JavaScript one**: a value outside ±2^53 is outside what the
language promises, and a program that round-trips through both backends behaves
the same only inside that range. Every other numeric type is its own width on
both backends.

**`usize` is Rust's `usize` here** — the platform word, which is what a native
`len()` is — with the same JavaScript guarantee: `usize::max_value()` answers
`9007199254740992` on both backends. Subtracting one past zero is unspecified,
and the backends really do differ: JS prints the negative number, a native
debug build panics with `attempt to subtract with overflow`, a release build
wraps, and rustc refuses a subtraction it can see underflows at compile time. So
a program that underflows is not one this backend promises to print the same
bytes for, and the corpus's `usize-underflow.vl` is named outside the
differential for exactly that reason. `checked_sub` and `saturating_sub` answer
the same on both.

**`BigInt` is an `i128` here, and that is a limit rather than a promise.** On
the JS backend a `BigInt` is arbitrary precision; natively it is a 128-bit
signed integer — ±170,141,183,460,469,231,731,687,303,715,884,105,727 — because
the runtime this backend links against takes no dependencies and so has no
bignum to lower one to. The limit is enforced at both ends rather than papered
over: a literal past it is **refused at compile time**, naming the value and the
range, and an operation that leaves the range **traps at run time** with the
same sentence instead of wrapping to a wrong number. Inside the range the two
backends agree exactly, printing included — node writes a `BigInt` with its
suffix, so `7n / 2n` is `3n` on both. If you need arbitrary precision as the
SUBJECT of a program — RSA, a big factorial — that is the JS backend today.

**Integer overflow is the program's, on both backends.** A conforming program
does not overflow (spec §7.2a), and the two backends answer one that does
differently: the JS backend runs on with an out-of-range value, and a native
debug build panics. An overflow rustc can see at compile time — `a + 1` right
after `let a: i32 = 2147483647;` — does not build at all, and `vilan` says so as
the program's overflow, naming the expression rustc underlined in the emitted
Rust, rather than as a backend defect.

## Where the emitted Rust goes

`dist/native/<entry>/`, with `src/main.rs` the emitted program and `Cargo.toml`
pointing at the runtime crate. It is ordinary Rust and you can read it; that is
the point of emitting a language rather than machine code. `vilan build
--backend rust --stdout` prints it without building.

`rm -rf dist` means the same thing here as everywhere else.

## Not here yet

Native **windows and UI** are not in this backend and are not close: how UI code
should be written once and run on both native and the web is a design question
being settled first.
