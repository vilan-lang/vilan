//! The A13 dev channel (hmr.md §§2, 6, and slice S1). A tiny HTTP endpoint
//! hand-rolled on `std::net::TcpListener`, bound to `127.0.0.1` only, in keeping
//! with the dependency-free watcher — Server-Sent Events need no websocket
//! handshake and no crate. It serves three routes:
//!
//! - `GET /events` — an SSE stream held open; on connect the current build
//!   version, then one event per watch round (`swap` / `css` / `reload` /
//!   `error`). The connection's own thread stays on it, blocked reading the
//!   socket, and unregisters it the moment the browser goes away ([`Clients`]).
//! - `GET /bundle/<leg>.js` and `GET /asset/<leg>.css` — the current artifacts
//!   from `dist/`, with `Access-Control-Allow-Origin: *`. Only bare
//!   `<name>.<ext>` names resolve; anything with a path separator or `..` is a
//!   404 (the traversal guard).
//! - `POST /refresh` — `std::watch::force_refresh()`'s wire target
//!   (`dev-refresh.md` §5 item 2; §6 records it as shipped): broadcasts a
//!   `reload` event to every connected browser and answers `204`.
//!
//! **Every one of them requires this run's token** (backlog E93). The listener
//! was always localhost-only and the artifact routes always had the traversal
//! guard, so the channel was never a network surface and never a file-read
//! primitive — but it was open to any page the developer happened to visit **in
//! the same browser** while `run --watch` ran. Such a page could read the
//! compiled bundle off `/bundle/<leg>.js`, hold `/events` open and watch every
//! compile failure scroll past as it was typed (`render_overlay` carries
//! `file:line:col`, the diagnostic text, and trace hops), and fire `POST
//! /refresh` for a reload flood — the port being a fixed, trivially probed
//! default. [`mint_token`] closes all three: the legitimate page has the token
//! baked into the bundle its own origin served it, and a hostile page cannot
//! read that bundle (the only copies are the local `dist/` and the app's origin,
//! which its own CORS protects) so it can neither fetch nor forge.
//!
//! `Access-Control-Allow-Origin: *` stays, and is load-bearing: the app's origin
//! is the user's own server and is genuinely unknowable here. An origin
//! allowlist would not have been an alternative anyway — a simple cross-origin
//! POST needs no CORS permission, so the refresh flood survives it and only a
//! secret the caller must present closes it.
//!
//! One residual, recorded rather than papered over: a tab left open from a
//! PREVIOUS watch session carries that session's token, so when the developer
//! restarts `run --watch` on the same port the old tab's `EventSource` is
//! refused instead of healing itself through the `connected` version gap. The
//! fix is to reload the tab. Answering a stale token any differently would mean
//! answering an unknown one, which is precisely what the gate exists to refuse —
//! and the shim must not reload on its own here, for the same reason
//! `fetchAndSwap` never does: a page whose own server hands out bytes it read
//! once at boot would loop forever.
//!
//! The browser side is [`SHIM`], a small dev-runtime prepended to browser-leg
//! bundles by an HMR-active `run --watch` (never by `build`, so goldens are
//! untouched). [`classify`] is the pure byte-diff decision the watch round runs
//! each rebuild; it is unit-tested without processes.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::net::TcpListener;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The default dev-channel port (hmr.md §2). `--hmr-port 0` asks the OS for an
/// ephemeral port instead.
pub const DEFAULT_HMR_PORT: u16 = 35917;

/// The dev-runtime shim, prepended to each browser leg's bundle when HMR is
/// active. Its `__VILAN_HMR_PORT__` / `__VILAN_HMR_TOKEN__` /
/// `__VILAN_HMR_VERSION__` / `__VILAN_HMR_BUNDLE__` placeholders are substituted
/// at write time by [`instrument`].
const SHIM: &str = include_str!("hmr_shim.js");

/// The dev-channel token's length in hex characters — 128 bits. A fixed,
/// non-secret constant: [`token_matches`] compares lengths in the clear, and
/// there is nothing to hide in a number every shipped shim carries anyway.
const TOKEN_HEX_LEN: usize = 32;

/// Mints this run's dev-channel token (backlog E93) — 128 bits, hex, generated
/// once per `run --watch` process and required on every route.
///
/// No RNG crate, in keeping with the dependency-free watcher: the token is a
/// SHA-256 digest (the `sha2` the upgrade path already pulls in) of four
/// independent unpredictable inputs, truncated to 128 bits.
///
/// The load-bearing input is `std::collections::hash_map::RandomState`, whose
/// keys the standard library seeds from the platform's own secure generator
/// (`getrandom` on Linux, `BCryptGenRandom` on Windows). A `RandomState`'s keys
/// cannot be read back, but hashing under it is keyed, so hashing two known
/// salts exports two key-dependent words. The standard library does not promise
/// that hasher is *cryptographic*, which is why it is not asked to be the whole
/// answer: wall-clock nanoseconds, the process id, and the address of a heap
/// allocation (ASLR) are mixed in beside it, and SHA-256 over the four is what
/// the token actually is. Even discounting the seeded keys entirely, what
/// remains is well past what a page that can only probe the port a few thousand
/// times a second could ever enumerate — and that page is the whole threat
/// model here.
fn mint_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut digest = Sha256::new();
    for salt in ["vilan-hmr-token-0", "vilan-hmr-token-1"] {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write(salt.as_bytes());
        digest.update(hasher.finish().to_le_bytes());
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since_epoch| since_epoch.as_nanos());
    digest.update(nanos.to_le_bytes());
    digest.update(std::process::id().to_le_bytes());
    let allocation = Box::new(0u8);
    digest.update((Box::as_ref(&allocation) as *const u8 as usize).to_le_bytes());
    let mut token = String::with_capacity(TOKEN_HEX_LEN);
    for byte in digest.finalize().iter().take(TOKEN_HEX_LEN / 2) {
        token.push_str(&format!("{byte:02x}"));
    }
    token
}

/// One browser leg's bundle, with the shim's port, this run's token, the build
/// version, and the leg's own bundle name embedded (the last so a `swap` event
/// can fetch `/bundle/<leg>.js`, hmr.md §3).
///
/// The token rides here and nowhere else on the browser side (backlog E93): the
/// bundle reaches the page over the page's OWN origin, which is exactly the
/// property that keeps a hostile page from reading it.
pub fn instrument(bundle: &str, port: u16, token: &str, version: u64, leg: &str) -> String {
    let shim = SHIM
        .replace("__VILAN_HMR_PORT__", &port.to_string())
        .replace("__VILAN_HMR_TOKEN__", token)
        .replace("__VILAN_HMR_VERSION__", &version.to_string())
        .replace("__VILAN_HMR_BUNDLE__", leg);
    format!("{shim}\n{bundle}")
}

/// The connected browsers, and the only place their sockets are registered,
/// written to, and dropped.
///
/// Every SSE connection is registered under an **id** rather than identified by
/// its socket, because two threads can decide it is finished: its own reader
/// (which sees the disconnect first, and without any traffic from us) and a
/// broadcast that fails to write to it. Removal by id is idempotent, so the
/// second one to arrive is a no-op instead of a race — and both go through the
/// one mutex, so neither can observe a half-updated registry.
#[derive(Default)]
struct Clients {
    connected: Mutex<Vec<Client>>,
    next_id: AtomicU64,
}

/// One connected browser.
struct Client {
    /// Identifies this connection for removal — see [`Clients`] on why the
    /// socket itself cannot serve as the key.
    id: u64,
    /// Shared with the connection's own reader thread, which watches the same
    /// socket for end-of-stream while watch rounds write to it. `&TcpStream`
    /// implements both `Read` and `Write`, so the two directions share one
    /// socket with no `try_clone` — which would have cost a second fd per
    /// browser to fix an fd leak.
    stream: Arc<TcpStream>,
}

impl Clients {
    /// Registers a freshly handshaken SSE stream, returning the id that
    /// [`Clients::remove`] takes it back out by.
    fn register(&self, stream: Arc<TcpStream>) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.connected.lock().unwrap().push(Client { id, stream });
        id
    }

    /// Drops one client. Idempotent: an id already gone (a broadcast pruned it
    /// first) removes nothing.
    fn remove(&self, id: u64) {
        self.connected
            .lock()
            .unwrap()
            .retain(|client| client.id != id);
    }

    /// Frames one payload as SSE and writes it to every connected client,
    /// dropping any whose socket has already failed.
    ///
    /// This prune is a backstop, not the reaper: a write to a socket the peer
    /// closed cleanly *succeeds* (the bytes reach the kernel; the RST that
    /// answers them only fails the write after it), so a dead client survives
    /// its first broadcast. The reader thread in [`serve_events`] is what
    /// actually keeps the registry honest.
    fn broadcast(&self, payload: &str) {
        let frame = sse_frame(payload);
        let mut connected = self.connected.lock().unwrap();
        connected.retain(|client| {
            let mut stream: &TcpStream = &client.stream;
            let alive = stream
                .write_all(frame.as_bytes())
                .and_then(|()| stream.flush())
                .is_ok();
            if !alive {
                // Wake this connection's reader thread: it is blocked on a
                // socket we have just given up on, and a shutdown returns it
                // from `read` at once instead of leaving it holding the fd.
                let _ = client.stream.shutdown(Shutdown::Both);
            }
            alive
        });
    }

    /// How many browsers are registered right now.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.connected.lock().unwrap().len()
    }
}

/// A live dev channel: an SSE server running on a background thread, plus the
/// shared build version the main watch thread bumps and embeds. Dropping it
/// leaves the accept thread — and one reader thread per *still-connected*
/// browser — parked on sockets that close when the process exits: a dev tool,
/// not a service to shut down cleanly.
pub struct DevChannel {
    port: u16,
    /// This run's token (backlog E93), minted at [`DevChannel::bind`] and
    /// required on every route. Shared with the accept loop's per-connection
    /// threads; never written anywhere but the instrumented bundle and the Node
    /// child's environment, and never printed.
    token: Arc<str>,
    version: Arc<AtomicU64>,
    clients: Arc<Clients>,
}

impl DevChannel {
    /// Binds the channel on `127.0.0.1:port` (port `0` ⇒ an OS-assigned
    /// ephemeral port) and spawns the accept loop. `dist` is the directory the
    /// artifact routes serve from. `Err` if the port is already in use — the
    /// caller warns and continues watching without HMR.
    pub fn bind(port: u16, dist: PathBuf) -> std::io::Result<DevChannel> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let port = listener.local_addr()?.port();
        let token: Arc<str> = Arc::from(mint_token());
        let version = Arc::new(AtomicU64::new(0));
        let clients: Arc<Clients> = Arc::new(Clients::default());
        {
            let token = token.clone();
            let version = version.clone();
            let clients = clients.clone();
            std::thread::spawn(move || serve(listener, clients, version, dist, token));
        }
        Ok(DevChannel {
            port,
            token,
            version,
            clients,
        })
    }

    /// How many browsers are connected right now. Observability for the
    /// reaping invariant (a disconnected client leaves the registry without a
    /// broadcast having to notice), which is otherwise invisible from outside.
    #[cfg(test)]
    fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// The bound port (the actual one when `0` was requested).
    pub fn port(&self) -> u16 {
        self.port
    }

    /// This run's token (backlog E93) — for the two places that legitimately
    /// need it: [`instrument`], which bakes it into each browser bundle, and the
    /// Node child's `VILAN_HMR_TOKEN`, which is how `std::watch::force_refresh()`
    /// reaches `POST /refresh`.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// This build's version — the counter embedded in the browser shim.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }

    /// Advances to the next build version (a new browser bundle was written).
    pub fn bump_version(&self) {
        self.version.fetch_add(1, Ordering::SeqCst);
    }

    /// Pushes one round event to every connected client. `message` is carried
    /// only by `error` events; `swap`/`reload` carry neither a message nor an
    /// asset. A `css` event names its sidecar — use [`DevChannel::push_css`].
    pub fn push(&self, kind: &str, message: Option<&str>) {
        self.broadcast(&event_json(kind, self.version(), message, None));
    }

    /// Pushes a `css` hot-swap event naming the changed sidecar file
    /// (`client.css`), so the shim bumps only the matching stylesheet `<link>`s
    /// (hmr.md §2). One event per changed sidecar keeps a multi-browser-leg
    /// workspace refreshing exactly the stylesheet that changed.
    pub fn push_css(&self, asset: &str) {
        self.broadcast(&event_json("css", self.version(), None, Some(asset)));
    }

    /// Frames one payload as SSE and writes it to every connected client.
    fn broadcast(&self, payload: &str) {
        self.clients.broadcast(payload);
    }
}

/// The SSE wire framing for one payload: `data: <json>\n\n`. The payload is
/// always single-line JSON (a message's newlines are `\n`-escaped by
/// [`escape_json`]), so the blank-line terminator is never ambiguous.
pub fn sse_frame(payload: &str) -> String {
    format!("data: {payload}\n\n")
}

/// One event's JSON body: `{"kind":..,"version":N}`, plus `"message":".."` for
/// an `error` (the rendered diagnostics), or `"asset":".."` for a `css` event
/// (the changed sidecar's filename). Both extra fields are escaped.
pub fn event_json(kind: &str, version: u64, message: Option<&str>, asset: Option<&str>) -> String {
    let mut body = format!("{{\"kind\":\"{kind}\",\"version\":{version}");
    if let Some(message) = message {
        body.push_str(&format!(",\"message\":\"{}\"", escape_json(message)));
    }
    if let Some(asset) = asset {
        body.push_str(&format!(",\"asset\":\"{}\"", escape_json(asset)));
    }
    body.push('}');
    body
}

/// Escapes a string for embedding in a JSON string literal (the diagnostic text
/// of an `error` event).
fn escape_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32))
            }
            other => out.push(other),
        }
    }
    out
}

/// The accept loop: one thread per connection (fine for a localhost dev tool).
/// A request-shaped route answers and the thread ends; an SSE connection keeps
/// its thread for as long as the browser is there ([`serve_events`]).
fn serve(
    listener: TcpListener,
    clients: Arc<Clients>,
    version: Arc<AtomicU64>,
    dist: PathBuf,
    token: Arc<str>,
) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let clients = clients.clone();
        let version = version.clone();
        let dist = dist.clone();
        let token = token.clone();
        std::thread::spawn(move || handle(stream, clients, version, dist, &token));
    }
}

/// Handles one connection: parse the request line, check this run's token,
/// ignore the remaining headers, route.
///
/// The token gate (backlog E93) sits **above the routing**, not inside each
/// route, and that placement is the point: there is no way to add a fifth route
/// that forgets it, and no request reaches the artifact reader, the client
/// registry, or the broadcast without having presented it first. The traversal
/// guard is unchanged and still runs underneath — a valid token buys the artifact
/// routes' `<name>.<ext>` discipline, not a path of the caller's choosing.
fn handle(
    mut stream: TcpStream,
    clients: Arc<Clients>,
    version: Arc<AtomicU64>,
    dist: PathBuf,
    token: &str,
) {
    let Some(request_line) = read_request_head(&mut stream) else {
        return;
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let (path, presented) = split_target(parts.next().unwrap_or(""));
    if !token_matches(token, presented) {
        respond_403(&mut stream);
        return;
    }
    if method == "POST" && path == "/refresh" {
        // `std::watch::force_refresh()`'s target (`dev-refresh.md` §5
        // item 2): a `reload` event, one-shot, to every connected browser.
        // The shim's never-reload doctrine (`fetchAndSwap`'s comment) is about
        // VERSION-GAP-triggered automatic reloads looping against a stale
        // server; this is neither automatic nor version-gap-driven — it is
        // one explicit call, and the reloaded page's shim has nothing left to
        // re-fire from it.
        let payload = event_json("reload", version.load(Ordering::SeqCst), None, None);
        clients.broadcast(&payload);
        respond_204(&mut stream);
        return;
    }
    if method != "GET" {
        respond_404(&mut stream);
        return;
    }
    if path == "/events" {
        serve_events(stream, &clients, &version);
        return;
    }
    if let Some(name) = path.strip_prefix("/bundle/") {
        serve_asset(&mut stream, &dist, name, "js");
        return;
    }
    if let Some(name) = path.strip_prefix("/asset/") {
        serve_asset(&mut stream, &dist, name, "css");
        return;
    }
    respond_404(&mut stream);
}

/// The `GET /events` route: Server-Sent Events, held open for the life of the
/// browser tab. Sends the current version immediately, registers the socket so
/// each watch round can push to it, and then **stays on this connection's
/// thread**, blocked reading the socket, until the browser goes away — at which
/// point it takes the client back out of the registry.
///
/// That last part is the whole reaping mechanism (backlog M3). An SSE client
/// never writes after its request head, so a `read` on this socket is pure
/// liveness: it blocks for as long as the tab lives and returns end-of-stream
/// the moment it does not. Leaving instead — which is what this route used to
/// do, dropping the connection's thread and keeping only its socket in the
/// registry — meant nothing on the server ever *looked* at a client until the
/// next round wrote to it, so a dev session with many reconnects (tab
/// refreshes, extra tabs, EventSource's own native reconnect) and sparse
/// rebuilds banked one dead fd per disconnect indefinitely.
///
/// The cost is one parked thread per *live* browser, where before there were
/// none — but the accept loop already spends a thread per connection, and this
/// only extends that thread's life to match the connection's rather than
/// spawning anything new. The thread exits with the tab.
fn serve_events(mut stream: TcpStream, clients: &Clients, version: &AtomicU64) {
    // `Access-Control-Allow-Origin: *` because the page origin is the user's
    // server, not the CLI.
    let headers = "HTTP/1.1 200 OK\r\n\
         Content-Type: text/event-stream\r\n\
         Cache-Control: no-cache\r\n\
         Connection: keep-alive\r\n\
         Access-Control-Allow-Origin: *\r\n\r\n";
    if stream.write_all(headers.as_bytes()).is_err() {
        return;
    }
    let hello = sse_frame(&event_json(
        "connected",
        version.load(Ordering::SeqCst),
        None,
        None,
    ));
    if stream.write_all(hello.as_bytes()).is_err() || stream.flush().is_err() {
        return;
    }
    let stream = Arc::new(stream);
    let id = clients.register(stream.clone());
    wait_for_disconnect(&stream);
    clients.remove(id);
}

/// Blocks until an SSE client's socket is finished — end-of-stream, an error,
/// or the shutdown a failed broadcast performs on it.
///
/// Anything the client does send is discarded: the browser has no reason to
/// write on this socket, and a byte that arrives anyway only proves the
/// connection is still alive, so the loop keeps watching. `Interrupted` is a
/// signal, not a disconnect, and is likewise not one.
fn wait_for_disconnect(stream: &TcpStream) {
    let mut reader: &TcpStream = stream;
    let mut discarded = [0u8; 256];
    loop {
        match reader.read(&mut discarded) {
            Ok(0) => return,
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
    }
}

/// Splits a request target into its path and the value of its `token` query
/// parameter (backlog E93). Every other parameter is ignored, and a target with
/// no query at all yields `None` — which [`token_matches`] refuses exactly as it
/// refuses a wrong one.
///
/// **Why the query string and not a header.** `EventSource` — the browser API
/// the shim's `/events` connection *is* — cannot set request headers, full stop,
/// so the SSE route has no choice. Given that, putting the other three in a
/// header would buy nothing and cost the thing that matters most for a security
/// check: one path, one parser, one place to get it wrong. A query parameter is
/// no weaker here either. The token is not a bearer credential loose on the
/// internet — it is loopback-only, it lives one `run --watch` process long, and
/// the URLs carrying it are built inside the shim and inside `force_refresh`,
/// never typed, linked, or logged. The attacker in the model is a page that
/// cannot read the token at all; where in the request it would have gone is
/// beside the point to them.
fn split_target(target: &str) -> (&str, Option<&str>) {
    match target.split_once('?') {
        None => (target, None),
        Some((path, query)) => (
            path,
            query
                .split('&')
                .find_map(|parameter| parameter.strip_prefix("token=")),
        ),
    }
}

/// Whether `presented` is this run's token. Absent, empty, short, long, or
/// simply wrong are one answer — `false` — and the caller renders all of them as
/// the same [`respond_403`].
///
/// The comparison runs over the whole token rather than stopping at the first
/// differing byte, so a caller cannot walk a guess in one character at a time off
/// response timing. The length check above it is not a leak: [`TOKEN_HEX_LEN`] is
/// a compile-time constant every shipped shim carries in the clear.
fn token_matches(expected: &str, presented: Option<&str>) -> bool {
    let Some(presented) = presented else {
        return false;
    };
    let expected = expected.as_bytes();
    let presented = presented.as_bytes();
    if expected.len() != presented.len() {
        return false;
    }
    let mut difference = 0u8;
    for (mine, theirs) in expected.iter().zip(presented) {
        difference |= mine ^ theirs;
    }
    difference == 0
}

/// Reads the whole HTTP request head (through the blank `\r\n\r\n` line) and
/// returns its first line (`GET /path HTTP/1.1`); the rest is ignored. Draining
/// the head matters: leaving unread request bytes in the socket buffer makes the
/// close after a response an RST instead of a FIN, which discards the not-yet-
/// delivered response body. A bounded read guards a client that never terminates
/// its head. `None` if the connection closed before any head arrived.
fn read_request_head(stream: &mut TcpStream) -> Option<String> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {
                head.push(byte[0]);
                if head.ends_with(b"\r\n\r\n") || head.len() > 16384 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    if head.is_empty() {
        return None;
    }
    String::from_utf8_lossy(&head)
        .lines()
        .next()
        .map(str::to_string)
}

/// Serves `dist/<name>` for an artifact route, with the traversal guard: only a
/// bare `<base>.<ext>` name (matching the route's extension) resolves; anything
/// with a path separator or `..` is a 404.
fn serve_asset(stream: &mut TcpStream, dist: &std::path::Path, name: &str, ext: &str) {
    if !is_safe_asset_name(name, ext) {
        respond_404(stream);
        return;
    }
    match std::fs::read(dist.join(name)) {
        Ok(bytes) => {
            let content_type = if ext == "js" {
                "text/javascript"
            } else {
                "text/css"
            };
            let header = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: {content_type}; charset=utf-8\r\n\
                 Content-Length: {}\r\n\
                 Access-Control-Allow-Origin: *\r\n\
                 Cache-Control: no-cache\r\n\r\n",
                bytes.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&bytes);
        }
        Err(_) => respond_404(stream),
    }
}

/// Whether `name` is a bare `<base>.<ext>` artifact name — no path separators,
/// no `..`, ending in the route's extension. The traversal guard.
pub fn is_safe_asset_name(name: &str, ext: &str) -> bool {
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return false;
    }
    let suffix = format!(".{ext}");
    // A bare `.js`/`.css` with no stem isn't a real artifact name either.
    name.ends_with(&suffix) && name.len() > suffix.len()
}

/// The token gate's refusal (backlog E93): one status, no body, and the same
/// bytes for every way of failing it.
///
/// `403` rather than `401` — there is no challenge to issue and no scheme to
/// name, so a `WWW-Authenticate` header would be a fiction; the request is simply
/// forbidden. Rather than `404` too: pretending the routes are not there would be
/// a lie the fixed default port already contradicts, and it would make a real
/// mistake (a stale bundle from a previous run) indistinguishable from a typo.
/// Nothing here says *why* — not whether a token was presented, not whether it
/// was the right length, not how far it matched.
fn respond_403(stream: &mut TcpStream) {
    let response = "HTTP/1.1 403 Forbidden\r\n\
         Content-Length: 0\r\n\
         Access-Control-Allow-Origin: *\r\n\r\n";
    let _ = stream.write_all(response.as_bytes());
}

fn respond_404(stream: &mut TcpStream) {
    let response = "HTTP/1.1 404 Not Found\r\n\
         Content-Length: 0\r\n\
         Access-Control-Allow-Origin: *\r\n\r\n";
    let _ = stream.write_all(response.as_bytes());
}

/// `POST /refresh`'s success response: no body to report.
fn respond_204(stream: &mut TcpStream) {
    let response = "HTTP/1.1 204 No Content\r\n\
         Access-Control-Allow-Origin: *\r\n\r\n";
    let _ = stream.write_all(response.as_bytes());
}

// --- The byte-diff classifier (hmr.md §§0.2, 6) ------------------------------

/// One build leg's artifacts for a round, as the classifier sees them: the
/// **raw** bundle bytes (before the shim is prepended — the shim embeds the
/// version, so shim-inclusive bytes differ every round) and the assembled CSS
/// sidecar content, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegArtifact {
    pub name: String,
    pub is_browser: bool,
    /// The extension this leg's bundle is written to `dist/` with, taken from
    /// its `Platform::script_extension` at compile time — a process leg gets
    /// `.mjs` so the runtime classifies it as ESM without sniffing, a browser
    /// leg keeps `.js`. Carried rather than re-derived so the watch round's
    /// writer and the restart's launcher cannot disagree about the name.
    pub script_extension: &'static str,
    pub bundle: String,
    pub css: Option<String>,
    /// The sources this artifact was compiled from — each loaded file's path
    /// mapped to the `content_hash` of the text the compiler actually consumed
    /// (`Program.source_hashes`). Covers the entry plus every std/package
    /// module the compile loaded; `macro_std` files pulled in only by macro
    /// *bodies* during world compilation are not listed (toolchain-development
    /// territory — user macro code lives in ordinary package modules, which
    /// are). Not part of the classifier — it compares `bundle`/`css` only.
    pub sources: BTreeMap<PathBuf, u64>,
}

/// Whether a leg's previous artifact is still current — the per-leg skip
/// decision (backlog E12, half b). True exactly when the leg's recorded source
/// map is non-empty and **every** source re-hashes, now, to the hash it was
/// compiled with. Reuse is decided by CONTENT, never by mtime: the watcher's
/// mtime scan only *triggers* rounds, and an mtime-preserving write, a
/// coarse-mtime filesystem, or a same-tick re-edit all still fail the re-hash
/// and recompile (the 2026-07-21 review's staleness finding). A deleted or
/// unreadable source yields `None` from `current_hash` and disqualifies the
/// skip; so does an empty map (no compile records zero sources — it means the
/// artifact predates source tracking).
pub fn leg_is_current(
    previous: &BTreeMap<PathBuf, u64>,
    current_hash: impl Fn(&Path) -> Option<u64>,
) -> bool {
    !previous.is_empty()
        && previous
            .iter()
            .all(|(path, hash)| current_hash(path) == Some(*hash))
}

/// The round-level guards that force a FULL recompile regardless of per-leg
/// verification (backlog E12): the first round (nothing recorded yet), a prior
/// failed round (its artifacts are not trustworthy), and a manifest change
/// (a `vilan.toml` can alter output without touching any `.vl` source).
/// Kept as a pure function so the safety cases stay pinned as logic.
pub fn round_forces_full(first_round: bool, prior_failed: bool, manifest_changed: bool) -> bool {
    first_round || prior_failed || manifest_changed
}

/// What the browser is told changed this round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Push {
    /// A browser bundle changed — reload (S1) / state-preserving swap (S2).
    Swap,
    /// Only CSS sidecar(s) changed — hot-swap those stylesheets, no reload. The
    /// payload names the changed sidecar files (`client.css`), so the shim bumps
    /// only the matching `<link>`s (hmr.md §2); a multi-browser-leg workspace
    /// thus refreshes exactly the stylesheet that changed.
    Css(Vec<String>),
}

/// The round's decision: whether to restart the Node child, what (if anything)
/// to push to browsers, and whether a new browser bundle means a version bump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundDecision {
    pub restart_server: bool,
    pub push: Option<Push>,
    pub bump_version: bool,
}

/// Classifies a rebuild by comparing this round's raw artifacts to the previous
/// round's (hmr.md §6):
///
/// - **first round** (`previous` empty) → spawn the child, bump to version 1,
///   push nothing (no clients yet);
/// - **the run server's bundle changed** → restart the child (and still classify
///   the client legs); a server-only change pushes nothing — K6 reconnect carries
///   the client across the restart;
/// - **browser bundle changed** → `swap`, and a version bump;
/// - **no bundle changed but a browser CSS sidecar changed** → `css`, naming the
///   changed sidecars;
/// - **nothing changed** → nothing, no restart.
///
/// `server_leg` is the ONE Node leg this `run --watch` executes (A15 entry
/// selection): a non-selected Node leg's bundle change drives no restart, because
/// that leg isn't run and isn't served to the browser — only its dist bytes are
/// refreshed. `None` when the workspace runs no Node leg (a browser-only dev
/// session, or an ambiguous multi-node choice deferred to the restart step).
///
/// A compile failure is handled by the caller (push `error`, retain the old
/// artifacts) and never reaches this classifier.
pub fn classify(
    previous: &[LegArtifact],
    next: &[LegArtifact],
    server_leg: Option<&str>,
) -> RoundDecision {
    if previous.is_empty() {
        return RoundDecision {
            restart_server: true,
            push: None,
            bump_version: true,
        };
    }
    let prior = |name: &str| previous.iter().find(|leg| leg.name == name);
    let is_server = |name: &str| server_leg == Some(name);
    let mut server_changed = false;
    let mut browser_changed = false;
    // The changed browser CSS sidecars, named for the `css` event so the shim
    // bumps exactly the matching `<link>`s (a node leg's CSS, if any, is not
    // linked by the page, so only browser sidecars participate).
    let mut changed_css: Vec<String> = Vec::new();
    let mut note_bundle_change = |leg: &LegArtifact| {
        if leg.is_browser {
            browser_changed = true;
        } else if is_server(&leg.name) {
            server_changed = true;
        }
    };
    for leg in next {
        match prior(&leg.name) {
            Some(old) => {
                if old.bundle != leg.bundle {
                    note_bundle_change(leg);
                }
                if old.css != leg.css && leg.is_browser {
                    changed_css.push(format!("{}.css", leg.name));
                }
            }
            // A newly-appearing leg is a change of its class.
            None => {
                note_bundle_change(leg);
                if leg.css.is_some() && leg.is_browser {
                    changed_css.push(format!("{}.css", leg.name));
                }
            }
        }
    }
    // A leg that vanished between rounds is likewise a change of its class.
    for old in previous {
        if !next.iter().any(|leg| leg.name == old.name) {
            if old.is_browser {
                browser_changed = true;
            } else if is_server(&old.name) {
                server_changed = true;
            }
        }
    }
    RoundDecision {
        restart_server: server_changed,
        push: if browser_changed {
            Some(Push::Swap)
        } else if !changed_css.is_empty() {
            Some(Push::Css(changed_css))
        } else {
            None
        },
        bump_version: browser_changed,
    }
}

// --- The error overlay's plain-text rendering (hmr.md §§2, 6) ----------------

/// The overlay caps at this many diagnostics before collapsing the tail to
/// "… and N more" — enough to see the shape of a broken build without an
/// unbounded event payload.
pub const OVERLAY_DIAGNOSTIC_CAP: usize = 20;

/// One diagnostic bound for the browser error overlay: the **file it belongs
/// to** with the 1-based location inside it, the already-built message (the SAME
/// text the terminal renders — an analyzer `error.msg`, or the parser's `render`
/// — this module never rebuilds it, only frames it), and an optional note.
///
/// The location is resolved at capture time rather than at render time (E16's
/// residual): a diagnostic's span indexes into *its own* file, so only the
/// caller — which holds every loaded source — can map it. Rendering then needs
/// no source text at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayDiagnostic {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub note: Option<String>,
    /// The diagnostic's requirement trace (backlog E78, surfaced here by E80),
    /// in the analyzer's entry → read order. Empty for every diagnostic but
    /// the context-coverage refusals.
    pub trace: Vec<OverlayTraceEntry>,
}

/// One entry of a requirement trace bound for the overlay: the hop's OWN file
/// with the 1-based location inside it, its label, and whether it marks an
/// uncovered call site. Located at capture like the diagnostic itself — a
/// chain hop in an importing module indexes the module's text
/// (`Note::source`), never the anchor's — so rendering needs no source.
///
/// `call` decides the line form: a call hop renders as `via file:line:col —
/// label`; the chain's elision tail (the one non-call entry, anchored at the
/// last kept hop's span) renders its text alone, so a location is never named
/// twice in a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayTraceEntry {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub call: bool,
}

impl OverlayDiagnostic {
    /// Captures a diagnostic against the source its span indexes into: `file`
    /// names that source and `text` is its content, so a module diagnostic
    /// resolves in the module and the entry's in the entry. Trace-free; a
    /// chain is attached with [`OverlayDiagnostic::with_trace`].
    pub fn located(
        file: &str,
        text: &str,
        span: std::ops::Range<usize>,
        message: String,
        note: Option<String>,
    ) -> OverlayDiagnostic {
        let (line, column) = line_col(text, span.start);
        OverlayDiagnostic {
            file: file.to_string(),
            line,
            column,
            message,
            note,
            trace: Vec::new(),
        }
    }

    /// Attaches the diagnostic's requirement trace, each entry already located
    /// in its own file.
    pub fn with_trace(mut self, trace: Vec<OverlayTraceEntry>) -> OverlayDiagnostic {
        self.trace = trace;
        self
    }
}

impl OverlayTraceEntry {
    /// Captures one hop against the source ITS span indexes into — which is
    /// the hop's `Note::source` when that names another file, and the
    /// diagnostic's own otherwise.
    pub fn located(
        file: &str,
        text: &str,
        span: std::ops::Range<usize>,
        message: String,
        call: bool,
    ) -> OverlayTraceEntry {
        let (line, column) = line_col(text, span.start);
        OverlayTraceEntry {
            file: file.to_string(),
            line,
            column,
            message,
            call,
        }
    }
}

/// Renders a failed leg's diagnostics as ANSI-free plain text for the `error`
/// event (the in-page overlay, hmr.md §§2/§6). A header line naming the leg,
/// then each diagnostic as `<file>:<line>:<col>` / `<message>`, its
/// requirement trace (E80) as one indented line per entry — `via
/// <file>:<line>:<col> — <label>` for a call hop, the elision tail's text
/// alone — and any note indented beneath, diagnostics separated by a blank
/// line. The trace sits between the message and the note in the analyzer's
/// entry → read order, the same trace-then-note order the terminal's labels
/// and the language server's related information keep. Each diagnostic names
/// **its own** file (E16), so a module error points at the module rather than
/// at the leg's entry, and each hop names ITS file. Capped at `cap`; the
/// remainder collapses to a trailing "… and N more". The terminal path is
/// untouched: this is a second, additive rendering of the same messages.
///
/// The line classes are the shim's contract (`hmr_shim.js`): a location line
/// is unindented `file:line:col`; a trace line is indented and begins with
/// `via ` or the tail's `…`; a note line is indented `note:`. A trace line
/// names a location too, so the shim classes it FIRST — it colours the
/// chain and never counts a hop toward the header badge.
pub fn render_overlay(leg_file: &str, diagnostics: &[OverlayDiagnostic], cap: usize) -> String {
    use std::fmt::Write;
    let shown = diagnostics.len().min(cap);
    let mut out = format!("{leg_file}: {} error", diagnostics.len());
    if diagnostics.len() != 1 {
        out.push('s');
    }
    for diagnostic in diagnostics.iter().take(cap) {
        // `file:line:col` on its own line (the shim styles it as a distinct
        // location line), then the message — the SAME text the terminal renders.
        let _ = write!(
            out,
            "\n\n{}:{}:{}\n{}",
            diagnostic.file, diagnostic.line, diagnostic.column, diagnostic.message
        );
        for hop in &diagnostic.trace {
            if hop.call {
                let _ = write!(
                    out,
                    "\n    via {}:{}:{} — {}",
                    hop.file, hop.line, hop.column, hop.message
                );
            } else {
                let _ = write!(out, "\n    {}", hop.message);
            }
        }
        if let Some(note) = &diagnostic.note {
            let _ = write!(out, "\n    note: {note}");
        }
    }
    let remaining = diagnostics.len() - shown;
    if remaining > 0 {
        let _ = write!(out, "\n\n… and {remaining} more");
    }
    out
}

/// The 1-based (line, column) of a byte offset in `src`, columns counted in
/// Unicode scalar values (as the terminal renderer does). Clamped to the source
/// length so an end-of-input span resolves to the last position.
fn line_col(src: &str, byte: usize) -> (usize, usize) {
    let byte = byte.min(src.len());
    let mut line = 1usize;
    let mut column = 1usize;
    for (index, character) in src.char_indices() {
        if index >= byte {
            break;
        }
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- The client registry's reaping (backlog M3) ---------------------------

    /// How long a registry assertion waits for the server's own thread to act.
    /// A liveness bound, not a performance claim: the reaping is a `read`
    /// returning end-of-stream, which costs nothing, so this only has to be too
    /// large for a healthy loopback socket and finite for a broken one. A green
    /// run never pays it — every wait returns the moment its count holds.
    const REGISTRY_LIVENESS: std::time::Duration = std::time::Duration::from_secs(30);

    /// Opens one SSE connection the way a browser's `EventSource` does —
    /// presenting this run's token, as every route now requires (backlog E93) —
    /// and reads through the `connected` hello, so the server has finished
    /// registering it by the time this returns.
    fn connect_sse(channel: &DevChannel) -> TcpStream {
        let mut stream =
            TcpStream::connect(("127.0.0.1", channel.port())).expect("connect to the dev channel");
        stream
            .set_read_timeout(Some(REGISTRY_LIVENESS))
            .expect("set a read timeout");
        write!(
            stream,
            "GET /events?token={} HTTP/1.1\r\nHost: localhost\r\n\r\n",
            channel.token()
        )
        .expect("send the SSE request");
        // The head, then the hello frame's `\n\n` terminator.
        let mut received = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            let read = stream.read(&mut chunk).expect("read the SSE handshake");
            assert!(read > 0, "the dev channel closed during the handshake");
            received.extend_from_slice(&chunk[..read]);
            if let Some(head_end) = received.windows(4).position(|window| window == b"\r\n\r\n")
                && received[head_end + 4..].windows(2).any(|w| w == b"\n\n")
            {
                break;
            }
        }
        stream
    }

    /// Waits (bounded) for the registry to hold exactly `expected` clients.
    fn assert_client_count(channel: &DevChannel, expected: usize, what: &str) {
        let start = std::time::Instant::now();
        while start.elapsed() < REGISTRY_LIVENESS {
            if channel.client_count() == expected {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!(
            "{what}: expected {expected} registered client(s), found {}",
            channel.client_count()
        );
    }

    #[test]
    fn a_disconnected_client_leaves_the_registry_without_a_broadcast() {
        // Backlog M3. The registry used to shed a client only as a side effect
        // of a later push failing to write to it, so a dev session that
        // reconnects often and rebuilds rarely banked one dead socket per
        // disconnect — 100 connect/disconnect cycles against a real
        // `run --watch` left 100 leaked fds, and the next rebuild reclaimed
        // none of them (a write to a cleanly closed peer succeeds; only the
        // one after it fails).
        //
        // Nothing here ever pushes an event. That is the point: reaping must
        // not depend on a broadcast noticing.
        let channel =
            DevChannel::bind(0, PathBuf::from("dist")).expect("bind an ephemeral dev channel");

        // Three generations of browser, so the claim is "returns to zero every
        // time", not "was zero once".
        for generation in 0..3 {
            let connections: Vec<TcpStream> = (0..4).map(|_| connect_sse(&channel)).collect();
            // Registration first — otherwise the count below could be zero
            // because nothing was ever registered.
            assert_client_count(
                &channel,
                4,
                &format!("generation {generation}: four live browsers"),
            );
            drop(connections);
            assert_client_count(
                &channel,
                0,
                &format!("generation {generation}: after every browser disconnected"),
            );
        }
    }

    #[test]
    fn a_live_client_stays_registered_and_receives_pushes() {
        // The other half of the invariant: the reader thread watching for a
        // disconnect must not itself unregister a healthy connection, and the
        // registry it now indexes by id still fans out.
        let channel =
            DevChannel::bind(0, PathBuf::from("dist")).expect("bind an ephemeral dev channel");
        let mut browser = connect_sse(&channel);
        assert_client_count(&channel, 1, "one live browser");

        channel.bump_version();
        channel.push("swap", None);

        let mut chunk = [0u8; 512];
        let read = browser.read(&mut chunk).expect("read the pushed event");
        let pushed = String::from_utf8_lossy(&chunk[..read]).into_owned();
        assert!(
            pushed.contains("\"kind\":\"swap\""),
            "the live browser should have received the pushed event: {pushed:?}"
        );
        assert_client_count(&channel, 1, "still one live browser after a push");
    }

    // --- The per-run token gate (backlog E93) --------------------------------

    /// A `dist/` with one browser bundle and its sidecar, for the artifact
    /// routes to actually serve. Returned with a live channel bound on it.
    fn channel_with_dist(tag: &str) -> (DevChannel, PathBuf) {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dist = std::env::temp_dir().join(format!(
            "vilan_hmr_token_{tag}_{}_{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dist);
        std::fs::create_dir_all(&dist).expect("create a dist directory");
        std::fs::write(dist.join("client.js"), "// bundle bytes\n").expect("write a bundle");
        std::fs::write(dist.join("client.css"), ".a{color:red}\n").expect("write a sidecar");
        let channel = DevChannel::bind(0, dist.clone()).expect("bind an ephemeral dev channel");
        (channel, dist)
    }

    /// One raw request against the dev channel, returning the WHOLE response —
    /// status line, headers, and body. A refusal has to be checked in full, not
    /// by its body alone, which is empty either way.
    fn raw_request(port: u16, method: &str, target: &str) -> String {
        let mut stream =
            TcpStream::connect(("127.0.0.1", port)).expect("connect to the dev channel");
        stream
            .set_read_timeout(Some(REGISTRY_LIVENESS))
            .expect("set a read timeout");
        write!(
            stream,
            "{method} {target} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n"
        )
        .expect("send the request");
        let mut response = Vec::new();
        let _ = stream.read_to_end(&mut response);
        String::from_utf8_lossy(&response).into_owned()
    }

    /// Every route, in the one place a test can name them all, as
    /// `(method, path)`. Adding a route to [`handle`] without adding it here is
    /// the omission these pins exist to catch.
    const ROUTES: [(&str, &str); 4] = [
        ("GET", "/events"),
        ("GET", "/bundle/client.js"),
        ("GET", "/asset/client.css"),
        ("POST", "/refresh"),
    ];

    #[test]
    fn every_route_answers_a_request_carrying_this_runs_token() {
        // The happy path, route by route: the token the shim would have been
        // given is the token the channel accepts. `/events` is checked through
        // `connect_sse`, which reads all the way through the hello — anything
        // less than a real registration would hang there rather than pass.
        let (channel, dist) = channel_with_dist("happy");
        let token = channel.token().to_string();

        let browser = connect_sse(&channel);
        assert_client_count(
            &channel,
            1,
            "GET /events with the token registers a browser",
        );
        drop(browser);

        let bundle = raw_request(
            channel.port(),
            "GET",
            &format!("/bundle/client.js?token={token}"),
        );
        assert!(
            bundle.starts_with("HTTP/1.1 200 OK") && bundle.ends_with("// bundle bytes\n"),
            "GET /bundle/<leg>.js with the token should serve the bundle: {bundle:?}"
        );

        let asset = raw_request(
            channel.port(),
            "GET",
            &format!("/asset/client.css?token={token}"),
        );
        assert!(
            asset.starts_with("HTTP/1.1 200 OK") && asset.ends_with(".a{color:red}\n"),
            "GET /asset/<leg>.css with the token should serve the sidecar: {asset:?}"
        );

        let refresh = raw_request(channel.port(), "POST", &format!("/refresh?token={token}"));
        assert!(
            refresh.starts_with("HTTP/1.1 204 No Content"),
            "POST /refresh with the token should broadcast and answer 204: {refresh:?}"
        );

        let _ = std::fs::remove_dir_all(&dist);
    }

    #[test]
    fn every_route_refuses_a_missing_or_wrong_token_with_the_same_bare_403() {
        // The refusal, route by route and for every way of failing the gate.
        // Byte-identical across all of them is the assertion that matters: a
        // caller learns that it did not present this run's token and nothing
        // else — not whether it presented one, not whether it was the right
        // length, not how far it matched, and not whether the route or the
        // artifact behind it exists.
        let (channel, dist) = channel_with_dist("refusal");
        let real = channel.token().to_string();
        let wrong: String = real
            .chars()
            .map(|digit| if digit == '0' { '1' } else { '0' })
            .collect();
        let attempts = [
            ("no query at all", String::new()),
            ("an empty token", "?token=".to_string()),
            ("another parameter but no token", "?nope=1".to_string()),
            (
                "a wrong token of the right length",
                format!("?token={wrong}"),
            ),
            (
                "a short token",
                format!("?token={}", &real[..real.len() - 1]),
            ),
            ("a long token", format!("?token={real}0")),
            (
                "the right token as some other parameter",
                format!("?bearer={real}"),
            ),
        ];

        let mut seen: Vec<String> = Vec::new();
        for (method, path) in ROUTES {
            for (what, query) in &attempts {
                let response = raw_request(channel.port(), method, &format!("{path}{query}"));
                assert_eq!(
                    response,
                    "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\
                     Access-Control-Allow-Origin: *\r\n\r\n",
                    "{method} {path} with {what} must be refused with a bare 403"
                );
                seen.push(response);
            }
        }
        assert!(
            seen.windows(2).all(|pair| pair[0] == pair[1]),
            "every refusal must be byte-identical — a difference is the leak"
        );
        // And nothing got through: no browser was registered by any of the
        // refused `/events` attempts.
        assert_client_count(&channel, 0, "a refused /events registers no browser");

        let _ = std::fs::remove_dir_all(&dist);
    }

    #[test]
    fn the_traversal_guard_still_holds_behind_the_token_gate() {
        // The gate sits ABOVE the routing, so a valid token is the only way to
        // reach the artifact reader at all — and reaching it buys the same
        // `<name>.<ext>` discipline as before, not a path of the caller's
        // choosing. Both halves are asserted: without the token a traversal is
        // a 403 (it never reaches the guard), with the token it is the guard's
        // own 404.
        let (channel, dist) = channel_with_dist("traversal");
        let token = channel.token().to_string();
        // A real file one level up, so a working traversal would have something
        // to hand back and an empty body cannot be mistaken for "nothing there".
        let secret = dist.parent().unwrap().join("vilan_hmr_token_secret.js");
        std::fs::write(&secret, "SECRET\n").expect("write the file a traversal would reach");

        for target in [
            "/bundle/../vilan_hmr_token_secret.js",
            "/asset/../vilan_hmr_token_secret.js",
            "/bundle/..%2fvilan_hmr_token_secret.js",
            "/bundle/sub/client.js",
            "/bundle/.js",
        ] {
            let refused = raw_request(channel.port(), "GET", target);
            assert!(
                refused.starts_with("HTTP/1.1 403 Forbidden"),
                "{target} with no token is refused before the guard is consulted: {refused:?}"
            );
            let guarded = raw_request(channel.port(), "GET", &format!("{target}?token={token}"));
            assert!(
                guarded.starts_with("HTTP/1.1 404 Not Found"),
                "{target} with the token must still hit the traversal guard: {guarded:?}"
            );
            assert!(
                !guarded.contains("SECRET"),
                "no traversal may serve bytes from outside dist: {guarded:?}"
            );
        }

        let _ = std::fs::remove_file(&secret);
        let _ = std::fs::remove_dir_all(&dist);
    }

    #[test]
    fn the_shim_carries_the_substituted_token_and_no_placeholder() {
        // The delivery half: the legitimate page gets the token because it is
        // baked into the bundle its own origin served it. A leftover placeholder
        // would be a shim that asks for `__VILAN_HMR_TOKEN__` and is refused by
        // every route.
        let token = mint_token();
        let instrumented = instrument("export const x = 1;\n", 35917, &token, 7, "client");
        assert!(
            instrumented.contains(&format!("var TOKEN = \"{token}\";")),
            "the shim should carry this run's token"
        );
        for placeholder in [
            "__VILAN_HMR_TOKEN__",
            "__VILAN_HMR_PORT__",
            "__VILAN_HMR_VERSION__",
            "__VILAN_HMR_BUNDLE__",
        ] {
            assert!(
                !instrumented.contains(placeholder),
                "{placeholder} should have been substituted"
            );
        }
        assert!(
            instrumented.ends_with("export const x = 1;\n"),
            "the app bundle should still follow the shim verbatim"
        );
    }

    #[test]
    fn a_token_is_128_hex_bits_and_differs_run_to_run() {
        // "Per-run" is the whole claim: a token that repeated across runs would
        // let a page that saw one dev session's bundle read the next one's.
        let tokens: Vec<String> = (0..64).map(|_| mint_token()).collect();
        for token in &tokens {
            assert_eq!(token.len(), TOKEN_HEX_LEN, "a token is 128 bits of hex");
            assert!(
                token.chars().all(|digit| digit.is_ascii_hexdigit()),
                "a token is hex, so it needs no URL escaping: {token}"
            );
        }
        let mut distinct = tokens.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            tokens.len(),
            "every minted token must differ — a repeat is a fixed secret"
        );
    }

    #[test]
    fn a_target_splits_into_its_path_and_its_token_parameter() {
        assert_eq!(split_target("/events"), ("/events", None));
        assert_eq!(split_target("/events?token=abc"), ("/events", Some("abc")));
        assert_eq!(split_target("/events?token="), ("/events", Some("")));
        // The parameter is found wherever it sits, and only under its own name.
        assert_eq!(
            split_target("/events?v=1&token=abc&w=2"),
            ("/events", Some("abc"))
        );
        assert_eq!(split_target("/events?bearer=abc"), ("/events", None));
        assert_eq!(split_target("/events?mytoken=abc"), ("/events", None));
        // The path never keeps the query — the traversal guard must not have to
        // reason about one.
        assert_eq!(
            split_target("/bundle/client.js?token=abc"),
            ("/bundle/client.js", Some("abc"))
        );
    }

    #[test]
    fn only_the_exact_token_matches() {
        // A literal rather than a minted one: these are exact-shape claims
        // (case, prefix, length), and they should read the same on every run.
        let token = "0123456789abcdef0123456789abcdef";
        assert!(token_matches(token, Some(token)));
        assert!(!token_matches(token, None));
        assert!(!token_matches(token, Some("")));
        assert!(!token_matches(token, Some(&token[..token.len() - 1])));
        assert!(!token_matches(token, Some(&format!("{token}0"))));
        // A guess that matches every character but the last is no closer than
        // one that matches none.
        assert!(!token_matches(
            token,
            Some("0123456789abcdef0123456789abcde0")
        ));
        assert!(!token_matches(
            token,
            Some("f0123456789abcdef0123456789abcde")
        ));
        // Hex, compared as bytes: a different case is a different token.
        assert!(!token_matches(token, Some(&token.to_uppercase())));
    }

    fn leg(name: &str, is_browser: bool, bundle: &str, css: Option<&str>) -> LegArtifact {
        LegArtifact {
            name: name.to_string(),
            is_browser,
            script_extension: if is_browser { "js" } else { "mjs" },
            bundle: bundle.to_string(),
            css: css.map(str::to_string),
            sources: BTreeMap::new(),
        }
    }

    fn round(server: &str, client: &str, css: Option<&str>) -> Vec<LegArtifact> {
        vec![
            leg("server", false, server, None),
            leg("client", true, client, css),
        ]
    }

    #[test]
    fn first_round_spawns_and_bumps_but_pushes_nothing() {
        // No previous artifacts: the child spawns, version becomes 1, and there
        // are no clients to push to yet.
        let decision = classify(&[], &round("s0", "c0", Some(".a{}")), Some("server"));
        assert_eq!(
            decision,
            RoundDecision {
                restart_server: true,
                push: None,
                bump_version: true,
            }
        );
    }

    #[test]
    fn server_only_change_restarts_and_pushes_nothing() {
        // The server bundle changed, the client didn't: restart the Node child,
        // but push nothing — the client is unaffected and K6 reconnect carries
        // it across the restart.
        let previous = round("s0", "c0", Some(".a{}"));
        let next = round("s1", "c0", Some(".a{}"));
        assert_eq!(
            classify(&previous, &next, Some("server")),
            RoundDecision {
                restart_server: true,
                push: None,
                bump_version: false,
            }
        );
    }

    #[test]
    fn client_only_change_pushes_swap_without_restart() {
        let previous = round("s0", "c0", Some(".a{}"));
        let next = round("s0", "c1", Some(".a{}"));
        assert_eq!(
            classify(&previous, &next, Some("server")),
            RoundDecision {
                restart_server: false,
                push: Some(Push::Swap),
                bump_version: true,
            }
        );
    }

    #[test]
    fn css_only_change_pushes_css_without_restart() {
        let previous = round("s0", "c0", Some(".a{}"));
        let next = round("s0", "c0", Some(".b{}"));
        assert_eq!(
            classify(&previous, &next, Some("server")),
            RoundDecision {
                restart_server: false,
                push: Some(Push::Css(vec!["client.css".to_string()])),
                bump_version: false,
            }
        );
    }

    #[test]
    fn server_and_client_change_restarts_and_swaps() {
        let previous = round("s0", "c0", Some(".a{}"));
        let next = round("s1", "c1", Some(".a{}"));
        assert_eq!(
            classify(&previous, &next, Some("server")),
            RoundDecision {
                restart_server: true,
                push: Some(Push::Swap),
                bump_version: true,
            }
        );
    }

    #[test]
    fn no_change_does_nothing() {
        let previous = round("s0", "c0", Some(".a{}"));
        let next = round("s0", "c0", Some(".a{}"));
        assert_eq!(
            classify(&previous, &next, Some("server")),
            RoundDecision {
                restart_server: false,
                push: None,
                bump_version: false,
            }
        );
    }

    #[test]
    fn a_bundle_change_outranks_a_simultaneous_css_change() {
        // When the browser bundle AND its css both change, the event is `swap`
        // (a reload subsumes the stylesheet refresh), not `css`.
        let previous = round("s0", "c0", Some(".a{}"));
        let next = round("s0", "c1", Some(".b{}"));
        assert_eq!(
            classify(&previous, &next, Some("server")).push,
            Some(Push::Swap)
        );
    }

    #[test]
    fn sse_framing_is_data_json_blank_line() {
        assert_eq!(
            sse_frame(&event_json("swap", 3, None, None)),
            "data: {\"kind\":\"swap\",\"version\":3}\n\n"
        );
        assert_eq!(
            sse_frame(&event_json("error", 4, Some("oops \"x\"\nline"), None)),
            "data: {\"kind\":\"error\",\"version\":4,\"message\":\"oops \\\"x\\\"\\nline\"}\n\n"
        );
    }

    #[test]
    fn a_css_event_names_its_sidecar() {
        // The recorded S1 residue closed: the `css` event carries the changed
        // sidecar's filename so the shim bumps only the matching `<link>`.
        assert_eq!(
            event_json("css", 5, None, Some("client.css")),
            "{\"kind\":\"css\",\"version\":5,\"asset\":\"client.css\"}"
        );
    }

    #[test]
    fn a_reload_event_carries_no_message_or_asset() {
        // force_refresh's wire shape (`dev-refresh.md` §5 item 2): a bare
        // `{"kind":"reload","version":N}`, same as `swap` — the shim's
        // `reload` case needs nothing more than the kind to fire a one-shot
        // `location.reload()`.
        assert_eq!(
            event_json("reload", 7, None, None),
            "{\"kind\":\"reload\",\"version\":7}"
        );
    }

    #[test]
    fn a_non_selected_node_leg_change_drives_no_restart() {
        // A15 + the byte-diff classifier: a workspace runs ONE node leg
        // (`server`); a sibling node leg (`probe`) that isn't run and isn't
        // served must not trigger a restart when only its bundle changes — the
        // page is unaffected, the run server unchanged.
        let previous = vec![
            leg("server", false, "s0", None),
            leg("probe", false, "p0", None),
            leg("client", true, "c0", None),
        ];
        let next = vec![
            leg("server", false, "s0", None),
            leg("probe", false, "p1", None), // only the non-run leg changed
            leg("client", true, "c0", None),
        ];
        assert_eq!(
            classify(&previous, &next, Some("server")),
            RoundDecision {
                restart_server: false,
                push: None,
                bump_version: false,
            }
        );
        // …while a change to the SELECTED server leg does restart.
        let next_server = vec![
            leg("server", false, "s1", None),
            leg("probe", false, "p0", None),
            leg("client", true, "c0", None),
        ];
        assert_eq!(
            classify(&previous, &next_server, Some("server")).restart_server,
            true
        );
    }

    #[test]
    fn a_per_sidecar_css_change_names_only_the_changed_browser_leg() {
        // Two browser legs each with a sidecar; only `admin`'s CSS changed → the
        // event names `admin.css` alone (per-sidecar behavior, hmr.md §2).
        let previous = vec![
            leg("client", true, "c0", Some(".a{}")),
            leg("admin", true, "a0", Some(".x{}")),
        ];
        let next = vec![
            leg("client", true, "c0", Some(".a{}")),
            leg("admin", true, "a0", Some(".y{}")),
        ];
        assert_eq!(
            classify(&previous, &next, None).push,
            Some(Push::Css(vec!["admin.css".to_string()]))
        );
    }

    #[test]
    fn the_overlay_renders_file_line_col_message_and_note() {
        // The overlay shows the REAL diagnostics: file:line:col, the message
        // (the same text the terminal renders), and a note where present.
        let src = "fun main() {\n\tlet x = broken\n}\n";
        // `broken` starts at byte 22 (line 2, column 10 — after "\tlet x = ",
        // the leading tab counting as one column).
        let diagnostics = vec![
            OverlayDiagnostic::located(
                "src/app.vl",
                src,
                22..28,
                "cannot find `broken` in this scope".to_string(),
                Some("declared nowhere".to_string()),
            ),
            OverlayDiagnostic::located("src/app.vl", src, 0..3, "second problem".to_string(), None),
        ];
        let rendered = render_overlay("src/app.vl", &diagnostics, OVERLAY_DIAGNOSTIC_CAP);
        assert_eq!(
            rendered,
            "src/app.vl: 2 errors\n\n\
             src/app.vl:2:10\n\
             cannot find `broken` in this scope\n\
             \x20   note: declared nowhere\n\n\
             src/app.vl:1:1\n\
             second problem"
        );
        // ANSI-free: the overlay is HTML text, never a terminal control stream.
        assert!(!rendered.contains('\x1b'));
    }

    #[test]
    fn the_overlay_renders_a_requirement_trace_between_the_message_and_the_note() {
        // E80: a diagnostic's E78 chain renders as one indented line per
        // entry, entry → read, AFTER the message and BEFORE the note — the
        // same trace-then-note order the terminal's labels and the language
        // server's related information keep. A call hop names its own
        // file:line:col behind `via`; the elision tail (the non-call entry)
        // is its text alone, since its span is the last kept hop's.
        let entry = "import pkg::common::banner;\n\nfun main() {\n\tbanner();\n}\n";
        let module = "fun banner(): str {\n\tcurrent.get()\n}\n";
        // `current.get()` starts at byte 21 of the MODULE (line 2, column 2);
        // `banner()` starts at byte 43 of the ENTRY (line 4, column 2).
        let diagnostic = OverlayDiagnostic::located(
            "src/common.vl",
            module,
            21..34,
            "context `current` is read here, but this code can be reached without an enclosing `run`"
                .to_string(),
            Some("`current` is declared here".to_string()),
        )
        .with_trace(vec![
            OverlayTraceEntry::located(
                "src/client.vl",
                entry,
                43..51,
                "the context requirement flows through this call".to_string(),
                true,
            ),
            OverlayTraceEntry::located(
                "src/client.vl",
                entry,
                43..51,
                "… 2 more uncovered calls on this path".to_string(),
                false,
            ),
        ]);
        let rendered = render_overlay("src/client.vl", &[diagnostic], OVERLAY_DIAGNOSTIC_CAP);
        assert_eq!(
            rendered,
            "src/client.vl: 1 error\n\n\
             src/common.vl:2:2\n\
             context `current` is read here, but this code can be reached without an enclosing `run`\n\
             \x20   via src/client.vl:4:2 — the context requirement flows through this call\n\
             \x20   … 2 more uncovered calls on this path\n\
             \x20   note: `current` is declared here"
        );
        // The shim's line-class contract: every trace line is indented, so
        // `/^\s+(via |…)/` classes it before the location regex can count it —
        // a chain never inflates the header badge (the shim e2e drives the
        // real count; this pins the text shape it relies on).
        for line in rendered
            .lines()
            .filter(|line| line.contains("via ") || line.starts_with("    …"))
        {
            assert!(
                line.starts_with("    via ") || line.starts_with("    …"),
                "a trace line must be indented for the shim to class it: {line:?}"
            );
        }
    }

    #[test]
    fn a_trace_hop_is_located_in_its_own_file() {
        // E16's rule, applied per hop: a chain hop in an importing module
        // resolves its line/column against THAT module's text, not the
        // anchor's — the anchor's text would put the same offset elsewhere.
        let entry = "import pkg::common::banner;\n\nfun main() {\n\tbanner();\n}\n";
        let module = "fun banner(): str {\n\tcurrent.get()\n}\n";
        let hop = OverlayTraceEntry::located(
            "src/client.vl",
            entry,
            43..51,
            "the context requirement flows through this call".to_string(),
            true,
        );
        assert_eq!(
            (hop.file.as_str(), hop.line, hop.column),
            ("src/client.vl", 4, 2)
        );
        assert_ne!(line_col(module, 43), (4, 2));
    }

    #[test]
    fn the_overlay_names_each_diagnostics_own_file() {
        // E16's residual: the leg's entry heads the overlay, but a diagnostic
        // that belongs to a MODULE names that module — and its line/column are
        // resolved against the module's own text, not the entry's.
        let entry = "import pkg::helper;\n\nfun main() {\n\thelper::go()\n}\n";
        let module = "fun go() {\n\tlet x = broken\n}\n";
        let diagnostics = vec![OverlayDiagnostic::located(
            "src/helper.vl",
            module,
            // `broken` starts at byte 20 of the MODULE (line 2, column 10).
            20..26,
            "cannot find `broken` in this scope".to_string(),
            None,
        )];
        let rendered = render_overlay("src/main.vl", &diagnostics, OVERLAY_DIAGNOSTIC_CAP);
        assert_eq!(
            rendered,
            "src/main.vl: 1 error\n\n\
             src/helper.vl:2:10\n\
             cannot find `broken` in this scope"
        );
        // The entry's own text would have put the same offset elsewhere — the
        // pin is that the module's text decided.
        assert_ne!(line_col(entry, 20), (2, 10));
    }

    #[test]
    fn the_overlay_caps_and_counts_the_remainder() {
        let src = "x\n";
        let diagnostics: Vec<OverlayDiagnostic> = (0..25)
            .map(|index| {
                OverlayDiagnostic::located("a.vl", src, 0..1, format!("problem {index}"), None)
            })
            .collect();
        let rendered = render_overlay("a.vl", &diagnostics, 20);
        assert!(rendered.starts_with("a.vl: 25 errors\n"));
        assert!(rendered.contains("a.vl:1:1\nproblem 0"));
        assert!(rendered.contains("a.vl:1:1\nproblem 19"));
        assert!(!rendered.contains("problem 20"));
        assert!(rendered.ends_with("… and 5 more"));
    }

    #[test]
    fn asset_names_reject_traversal() {
        assert!(is_safe_asset_name("client.js", "js"));
        assert!(is_safe_asset_name("server.css", "css"));
        assert!(!is_safe_asset_name("../secret.js", "js"));
        assert!(!is_safe_asset_name("sub/client.js", "js"));
        assert!(!is_safe_asset_name("client.css", "js")); // wrong extension
        assert!(!is_safe_asset_name(".js", "js")); // no stem
        assert!(!is_safe_asset_name("..", "js"));
    }

    // --- The per-leg skip decision (backlog E12, half b) ---------------------

    fn recorded(entries: &[(&str, u64)]) -> BTreeMap<PathBuf, u64> {
        entries
            .iter()
            .map(|(name, hash)| (PathBuf::from(name), *hash))
            .collect()
    }

    /// A verifier over a fixed "current filesystem": present files hash to the
    /// listed value, everything else reads as unreadable.
    fn on_disk(entries: &[(&str, u64)]) -> impl Fn(&Path) -> Option<u64> {
        let current: BTreeMap<PathBuf, u64> = recorded(entries);
        move |path: &Path| current.get(path).copied()
    }

    #[test]
    fn an_unchanged_leg_verifies_and_is_current() {
        let previous = recorded(&[("server.vl", 1), ("std/list.vl", 2), ("common.vl", 3)]);
        let disk = on_disk(&[("server.vl", 1), ("std/list.vl", 2), ("common.vl", 3)]);
        assert!(leg_is_current(&previous, disk));
    }

    #[test]
    fn a_content_drift_disqualifies_the_leg_regardless_of_any_other_signal() {
        // The review's staleness finding, pinned: reuse is decided by CONTENT.
        // No mtime, changed-set, or watcher signal participates — if a source's
        // bytes differ from what was compiled (an mtime-preserving write, a
        // coarse-mtime filesystem, a same-tick re-edit), the re-hash fails and
        // the leg recompiles.
        let previous = recorded(&[("client.vl", 1), ("common.vl", 3)]);
        let disk = on_disk(&[("client.vl", 9), ("common.vl", 3)]);
        assert!(!leg_is_current(&previous, disk));
    }

    #[test]
    fn an_unreadable_or_deleted_source_disqualifies_the_leg() {
        let previous = recorded(&[("client.vl", 1), ("gone.vl", 2)]);
        let disk = on_disk(&[("client.vl", 1)]);
        assert!(!leg_is_current(&previous, disk));
    }

    #[test]
    fn an_empty_source_map_never_qualifies() {
        // No compile records zero sources — an empty map means the artifact
        // predates source tracking, and trusting it would skip on nothing.
        let previous = recorded(&[]);
        let disk = on_disk(&[]);
        assert!(!leg_is_current(&previous, disk));
    }

    #[test]
    fn a_leg_only_verifies_against_its_own_sources() {
        // A drift in a file the leg never loaded is invisible to it — the
        // client's edit leaves the server current (the client-only-edit skip).
        let server = recorded(&[("server.vl", 1), ("common.vl", 3)]);
        let disk = on_disk(&[("server.vl", 1), ("common.vl", 3), ("client.vl", 42)]);
        assert!(leg_is_current(&server, disk));
    }

    #[test]
    fn the_round_guards_force_a_full_recompile() {
        // The three safety cases that bypass per-leg verification entirely.
        assert!(round_forces_full(true, false, false), "first round");
        assert!(round_forces_full(false, true, false), "prior failure");
        assert!(round_forces_full(false, false, true), "manifest change");
        assert!(!round_forces_full(false, false, false));
    }
}
