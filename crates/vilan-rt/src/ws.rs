//! `vilan-rt::ws` — a WebSocket CLIENT over `std::net`, by hand (tracker F1
//! S1b; `proposal/native-objective.md` §3 S1).
//!
//! # What this is a twin OF
//!
//! `vilan/std/src/rpc.vl` binds the host `WebSocket` class — the constructor
//! with a subprotocol list, `send` (text and bytes), `binaryType`, the
//! `onmessage`/`onclose`/`onerror` handlers and `close()` — plus the message
//! event's `data`. Those bindings ARE the contract: every public item here
//! exists because one of them does, and each one behaves as the WHATWG
//! `WebSocket` behaves at that seam (undici's, under node), so `std::rpc`'s
//! dial, its reconnect loop and the generated client run from the same source
//! on both backends.
//!
//! # Why by hand
//!
//! `vilan-rt` takes no crates.io dependencies (Order 37's R8, Order 39's R1).
//! RFC 6455's client half is an HTTP/1.1 upgrade request, a SHA-1 check of one
//! header ([`crate::crypto::sha1`] and [`crate::crypto::base64`], which the
//! server half's accept key already uses) and a length-prefixed frame format,
//! so it fits inside that rule as the server did.
//!
//! # Not here, by name
//!
//! TLS: `wss://` (and `https://`) is refused at the constructor with a
//! sentence that says so (`native-objective.md` S1 — a native client dials
//! `ws://`, behind a TLS-terminating proxy when the server needs one). No
//! extension is offered (no `permessage-deflate`), so a server that answers
//! with one fails the handshake, as RFC 6455 §4.1 requires.
//!
//! # How it runs
//!
//! The constructor answers at once with a CONNECTING socket, as the host's
//! does; the address lookup and the TCP connect happen on a helper thread,
//! because `std::net` has no non-blocking connect and a blocking one would
//! stall the event loop (a DNS lookup or an unanswered SYN can take seconds).
//! Everything after the connect is the socket polled once per executor turn as
//! an [`IoSource`], non-blocking, like the server's connections.
//!
//! **One event per turn.** The host delivers each message, error and close as
//! its own task, with the microtask queue drained in between — so the dial's
//! continuation after the `__conn:` announcement runs before the next message
//! is handled. A poll here therefore hands at most ONE event to the program and
//! leaves the rest in its inbox for the next turn.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::hash::{BuildHasher, Hasher};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use crate::bytes::Bytes;
use crate::executor::{Interest, IoSource, loop_waker, register_io};
use crate::json::JsonValue;
use crate::{Js, Str, str_new};

/// RFC 6455 §1.3's GUID, appended to the key before the accept hash.
const ACCEPT_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// One read of the socket — node's default highWaterMark, as the server's.
const READ_CHUNK: usize = 8 * 1024;

/// The largest handshake response head this reads before failing the
/// connection: the head is buffered whole before it can be parsed.
const MAX_HEAD: usize = 64 * 1024;

/// How long `close()` on an open socket waits for the server's answering Close
/// frame before it drops the connection anyway (RFC 6455 §7.1.1 leaves the
/// bound to the client; a server that never answers must not hold the socket
/// open for good).
const CLOSE_GRACE: Duration = Duration::from_secs(5);

const OPCODE_CONTINUATION: u8 = 0x0;
const OPCODE_TEXT: u8 = 0x1;
const OPCODE_BINARY: u8 = 0x2;
const OPCODE_CLOSE: u8 = 0x8;
const OPCODE_PING: u8 = 0x9;
const OPCODE_PONG: u8 = 0xA;

/// RFC 6455 §7.4.1's status codes this side sends.
const STATUS_PROTOCOL_ERROR: u16 = 1002;
const STATUS_INVALID_PAYLOAD: u16 = 1007;

// ------------------------------------------------------------------ url ---

/// A `ws://` URL taken apart: where to connect and what to ask for.
#[derive(Debug, PartialEq)]
struct Target {
    /// The host as the connect reads it — an IPv6 literal without brackets.
    host: String,
    port: u16,
    /// The `Host` header's value: the host as written, with the port when it
    /// is not the scheme's default.
    authority: String,
    /// The request target: the path (at least `/`) and the query.
    resource: String,
}

/// Why a URL cannot be dialled — each the constructor's `SyntaxError` on the
/// host, and TLS this backend's own refusal.
#[derive(Debug, PartialEq)]
enum UrlError {
    Tls,
    Invalid(&'static str),
}

/// Parses `url` as the WebSocket constructor does: `ws:` (or `http:`, which
/// the host maps to it), a host, an optional port, a path and a query; a
/// fragment is refused.
fn parse_url(url: &str) -> Result<Target, UrlError> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or(UrlError::Invalid("the URL has no scheme"))?;
    match scheme.to_ascii_lowercase().as_str() {
        "ws" | "http" => {}
        "wss" | "https" => return Err(UrlError::Tls),
        _ => return Err(UrlError::Invalid("the URL's scheme must be `ws`")),
    }
    if rest.contains('#') {
        return Err(UrlError::Invalid(
            "a WebSocket URL may not carry a fragment",
        ));
    }
    let split = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(split);
    if authority.contains('@') {
        return Err(UrlError::Invalid(
            "a WebSocket URL may not carry credentials",
        ));
    }
    let (host, port_text) = if let Some(bracketed) = authority.strip_prefix('[') {
        let (literal, after) = bracketed
            .split_once(']')
            .ok_or(UrlError::Invalid("an unclosed IPv6 literal"))?;
        let port = match after {
            "" => None,
            _ => Some(
                after
                    .strip_prefix(':')
                    .ok_or(UrlError::Invalid("junk after the IPv6 literal"))?,
            ),
        };
        (literal.to_string(), port)
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host.to_string(), Some(port)),
            None => (authority.to_string(), None),
        }
    };
    if host.is_empty() {
        return Err(UrlError::Invalid("the URL has no host"));
    }
    let port = match port_text {
        None | Some("") => 80,
        Some(text) => text
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or(UrlError::Invalid("the URL's port is not a port"))?,
    };
    let written_host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_ascii_lowercase()
    };
    let authority = if port == 80 {
        written_host
    } else {
        format!("{written_host}:{port}")
    };
    let resource = if tail.starts_with('/') {
        tail.to_string()
    } else {
        format!("/{tail}")
    };
    Ok(Target {
        host: host.to_ascii_lowercase(),
        port,
        authority,
        resource,
    })
}

// --------------------------------------------------------------- random ---

thread_local! {
    /// The keyed generator behind the handshake key and the frame masks.
    ///
    /// RFC 6455 wants both unpredictable (§4.1, §5.3) and this crate has no
    /// OS randomness (that is `vilan-rt-crypto`'s `getrandom`, a dependency).
    /// std's `RandomState` IS seeded from the operating system, once per
    /// thread, so SipHash under its keys over a counter is a keyed
    /// pseudorandom stream nobody outside the process can predict — which is
    /// the property the masks exist for (keeping a client's bytes from being
    /// chosen by a script, against a proxy's cache), with no dependency.
    static RANDOM: (std::collections::hash_map::RandomState, Cell<u64>) =
        (std::collections::hash_map::RandomState::new(), Cell::new(0));
}

/// `count` unpredictable bytes.
fn random_bytes(count: usize) -> Vec<u8> {
    RANDOM.with(|(state, counter)| {
        let mut bytes = Vec::with_capacity(count + 8);
        while bytes.len() < count {
            let mut hasher = state.build_hasher();
            hasher.write_u64(counter.get());
            counter.set(counter.get().wrapping_add(1));
            bytes.extend_from_slice(&hasher.finish().to_le_bytes());
        }
        bytes.truncate(count);
        bytes
    })
}

// ---------------------------------------------------------------- frames ---

/// One client frame: FIN set, `opcode`, the payload MASKED under a fresh key
/// (RFC 6455 §5.2–5.3: every frame a client sends is masked).
fn client_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 14);
    frame.push(0x80 | opcode);
    let length = payload.len();
    if length < 126 {
        frame.push(0x80 | length as u8);
    } else if length <= usize::from(u16::MAX) {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(length as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(length as u64).to_be_bytes());
    }
    let mask = random_bytes(4);
    frame.extend_from_slice(&mask);
    frame.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4]),
    );
    frame
}

/// One frame read off the wire.
#[derive(Debug, PartialEq)]
struct Frame {
    fin: bool,
    opcode: u8,
    payload: Vec<u8>,
}

/// What [`take_frame`] found at the front of the inbox.
#[derive(Debug, PartialEq)]
enum Taken {
    /// A whole frame, removed from the inbox.
    Frame(Frame),
    /// Not a whole frame yet.
    More,
    /// A frame no server may send: the connection fails with this status.
    Fault(u16),
}

/// Takes one SERVER frame off the front of `inbox` (RFC 6455 §5.2). A server
/// frame is never masked, its reserved bits are clear (no extension was
/// negotiated), and a control frame is short and unfragmented.
fn take_frame(inbox: &mut Vec<u8>) -> Taken {
    if inbox.len() < 2 {
        return Taken::More;
    }
    let first = inbox[0];
    let second = inbox[1];
    let fin = first & 0x80 != 0;
    let opcode = first & 0x0F;
    if first & 0x70 != 0 || second & 0x80 != 0 {
        return Taken::Fault(STATUS_PROTOCOL_ERROR);
    }
    let is_control = opcode & 0x8 != 0;
    match opcode {
        OPCODE_CONTINUATION | OPCODE_TEXT | OPCODE_BINARY => {}
        OPCODE_CLOSE | OPCODE_PING | OPCODE_PONG => {}
        _ => return Taken::Fault(STATUS_PROTOCOL_ERROR),
    }
    let (length, header) = match second & 0x7F {
        126 => {
            if inbox.len() < 4 {
                return Taken::More;
            }
            (u64::from(u16::from_be_bytes([inbox[2], inbox[3]])), 4)
        }
        127 => {
            if inbox.len() < 10 {
                return Taken::More;
            }
            let mut wide = [0u8; 8];
            wide.copy_from_slice(&inbox[2..10]);
            (u64::from_be_bytes(wide), 10)
        }
        short => (u64::from(short), 2),
    };
    if is_control && (!fin || length > 125) {
        return Taken::Fault(STATUS_PROTOCOL_ERROR);
    }
    let Ok(length) = usize::try_from(length) else {
        return Taken::Fault(STATUS_PROTOCOL_ERROR);
    };
    if inbox.len() - header < length {
        return Taken::More;
    }
    let payload = inbox[header..header + length].to_vec();
    inbox.drain(..header + length);
    Taken::Frame(Frame {
        fin,
        opcode,
        payload,
    })
}

// ------------------------------------------------------------- handshake ---

/// The upgrade request (RFC 6455 §4.1).
fn upgrade_request(target: &Target, key: &str, protocols: &[Str]) -> Vec<u8> {
    let mut request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n",
        target.resource, target.authority
    );
    if !protocols.is_empty() {
        let offered: Vec<&str> = protocols.iter().map(|protocol| &**protocol).collect();
        request.push_str(&format!(
            "Sec-WebSocket-Protocol: {}\r\n",
            offered.join(", ")
        ));
    }
    request.push_str("\r\n");
    request.into_bytes()
}

/// The `Sec-WebSocket-Accept` a server must answer `key` with.
fn accept_for(key: &str) -> String {
    crate::crypto::base64(&crate::crypto::sha1(
        format!("{key}{ACCEPT_GUID}").as_bytes(),
    ))
}

/// Where the response head ends (the blank line), if it has arrived.
fn head_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|start| start + 4)
}

/// Checks a handshake response head against what the client asked for (RFC
/// 6455 §4.1's client requirements, as undici checks them): a `101`, the
/// `Upgrade`/`Connection` pair, the accept key, a subprotocol only out of the
/// offered list, and no extension.
fn verify_response(head: &[u8], key: &str, protocols: &[Str]) -> Result<(), String> {
    let text = std::str::from_utf8(head).map_err(|_| "the response head is not text")?;
    let mut lines = text.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next().unwrap_or_default();
    let status = parts.next().unwrap_or_default();
    if !version.starts_with("HTTP/1.") {
        return Err(format!("the response is not HTTP/1.1 ({status_line:?})"));
    }
    if status != "101" {
        return Err(format!("the server answered {status} instead of 101"));
    }
    let mut upgrade = None;
    let mut connection = None;
    let mut accept = None;
    let mut protocol = None;
    let mut extensions = None;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(format!("a malformed response header ({line:?})"));
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "upgrade" => upgrade = Some(value),
            "connection" => connection = Some(value),
            "sec-websocket-accept" => accept = Some(value),
            "sec-websocket-protocol" => protocol = Some(value),
            "sec-websocket-extensions" => extensions = Some(value),
            _ => {}
        }
    }
    if !upgrade.is_some_and(|value| value.eq_ignore_ascii_case("websocket")) {
        return Err("the response does not upgrade to websocket".to_string());
    }
    if !connection.is_some_and(|value| {
        value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
    }) {
        return Err("the response's Connection header does not say Upgrade".to_string());
    }
    if accept != Some(accept_for(key).as_str()) {
        return Err("the server's Sec-WebSocket-Accept does not match the key".to_string());
    }
    if let Some(selected) = protocol
        && !protocols.iter().any(|offered| &**offered == selected)
    {
        return Err(format!(
            "the server selected the subprotocol {selected:?}, which was not offered"
        ));
    }
    if extensions.is_some_and(|value| !value.is_empty()) {
        return Err("the server negotiated an extension that was not offered".to_string());
    }
    Ok(())
}

// --------------------------------------------------------------- client ---

/// The socket's `readyState`, plus the TCP half of CONNECTING split out.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    /// Waiting on the helper thread's lookup and connect.
    Connecting,
    /// Connected; the upgrade request is going out and the answer coming in.
    Handshaking,
    Open,
    /// `close()` sent a Close frame; waiting for the server's.
    Closing,
    Closed,
}

/// One event for the program — delivered one per turn ([`Client::poll`]).
enum Event {
    Message(SocketMessage),
    Error,
    Close,
}

/// The socket's whole state.
struct Client {
    stage: Cell<Stage>,
    /// The helper thread's answer while [`Stage::Connecting`].
    connecting: RefCell<Option<Receiver<std::io::Result<TcpStream>>>>,
    stream: RefCell<Option<TcpStream>>,
    key: String,
    protocols: Vec<Str>,
    /// The upgrade request, written once the TCP connect lands.
    request: RefCell<Vec<u8>>,
    inbox: RefCell<Vec<u8>>,
    outbox: RefCell<Vec<u8>>,
    /// A message arriving in fragments: its opcode and what has come so far.
    fragments: RefCell<Option<(u8, Vec<u8>)>>,
    events: RefCell<VecDeque<Event>>,
    on_message: RefCell<Option<Rc<dyn Fn(SocketMessage)>>>,
    on_close: RefCell<Option<Rc<dyn Fn()>>>,
    on_error: RefCell<Option<Rc<dyn Fn()>>>,
    /// The peer closed its end (EOF or a reset): what is in the inbox is all
    /// there will be.
    peer_gone: Cell<bool>,
    /// Shut the TCP connection once the outbox has drained (after a Close
    /// frame went out).
    shut_when_flushed: Cell<bool>,
    /// When a `close()` stops waiting for the server's Close frame.
    close_deadline: Cell<Option<Instant>>,
}

/// `WebSocket` — the host class `std::rpc` dials with.
#[derive(Clone)]
pub struct WebSocket(Rc<Client>);

impl WebSocket {
    /// `new WebSocket(url, protocols)`.
    ///
    /// Answers a CONNECTING socket at once and connects in the background. A
    /// URL the host would refuse with a `SyntaxError` panics here with the
    /// reason (the host's throw), and so does `wss://`: TLS is not in this
    /// backend (`native-objective.md` S1).
    pub fn connect(url: &str, protocols: &[Str]) -> WebSocket {
        let target = match parse_url(url) {
            Ok(target) => target,
            Err(UrlError::Tls) => crate::panic_with(&format!(
                "cannot dial `{url}`: the native backend has no TLS, so `wss://` is refused — \
                 dial `ws://` (behind a TLS-terminating proxy when the server needs one), or \
                 build with `--backend js`"
            )),
            Err(UrlError::Invalid(reason)) => crate::panic_with(&format!(
                "SyntaxError: Failed to construct 'WebSocket': {reason} (`{url}`)"
            )),
        };
        let key = crate::crypto::base64(&random_bytes(16));
        let request = upgrade_request(&target, &key, protocols);
        let (sender, receiver) = channel();
        let host = target.host.clone();
        let port = target.port;
        // The lookup and the connect block, so they run off the loop. If the
        // socket is closed first the receiver is gone, the send fails, and the
        // connected stream is dropped (closed) on this thread. The answer
        // wakes the loop (F160): nothing about it is a socket the loop could
        // be watching yet.
        let waker = loop_waker();
        std::thread::spawn(move || {
            let _ = sender.send(TcpStream::connect((host.as_str(), port)));
            waker.wake();
        });
        let client = Rc::new(Client {
            stage: Cell::new(Stage::Connecting),
            connecting: RefCell::new(Some(receiver)),
            stream: RefCell::new(None),
            key,
            protocols: protocols.to_vec(),
            request: RefCell::new(request),
            inbox: RefCell::new(Vec::new()),
            outbox: RefCell::new(Vec::new()),
            fragments: RefCell::new(None),
            events: RefCell::new(VecDeque::new()),
            on_message: RefCell::new(None),
            on_close: RefCell::new(None),
            on_error: RefCell::new(None),
            peer_gone: Cell::new(false),
            shut_when_flushed: Cell::new(false),
            close_deadline: Cell::new(None),
        });
        register_io(Rc::clone(&client) as Rc<dyn IoSource>);
        WebSocket(client)
    }

    /// `socket.send(text)`. Before the socket opens it throws, as the host's
    /// does; once it is closing or closed the message is discarded, as the
    /// host discards it.
    pub fn transmit(&self, data: &str) {
        self.0.send(OPCODE_TEXT, data.as_bytes());
    }

    /// `socket.send(bytes)`.
    pub fn transmit_bytes(&self, data: &Bytes) {
        self.0.send(OPCODE_BINARY, &data.as_slice());
    }

    /// `socket.binaryType = kind`. Binary messages always arrive as an
    /// `ArrayBuffer` natively — the only kind `std::rpc` asks for — so the
    /// setting is accepted and read by nothing.
    pub fn set_binary_type(&self, kind: &str) {
        let _ = kind;
    }

    /// `socket.onmessage = handler`.
    pub fn set_on_message(&self, handler: Rc<dyn Fn(SocketMessage)>) {
        *self.0.on_message.borrow_mut() = Some(handler);
    }

    /// `socket.onclose = handler`.
    pub fn set_on_close(&self, handler: Rc<dyn Fn()>) {
        *self.0.on_close.borrow_mut() = Some(handler);
    }

    /// `socket.onerror = handler`.
    pub fn set_on_error(&self, handler: Rc<dyn Fn()>) {
        *self.0.on_error.borrow_mut() = Some(handler);
    }

    /// `socket.close()`.
    ///
    /// CONNECTING: the connection is FAILED — the connect abandoned (a TCP
    /// connection already made is shut, so the server sees it go) and `error`
    /// then `close` delivered. OPEN: the closing handshake starts — a Close
    /// frame goes out and `close` is delivered when the server's answers, or
    /// after [`CLOSE_GRACE`]. Already closing or closed: nothing.
    pub fn close(&self) {
        let client = &self.0;
        match client.stage.get() {
            Stage::Connecting | Stage::Handshaking => client.fail(None),
            Stage::Open => {
                // No code and no reason: the host's `close()` sends a Close
                // frame with no body.
                client.queue_bytes(&client_frame(OPCODE_CLOSE, &[]));
                client.stage.set(Stage::Closing);
                client
                    .close_deadline
                    .set(Some(Instant::now() + CLOSE_GRACE));
            }
            Stage::Closing | Stage::Closed => {}
        }
    }
}

impl Client {
    fn send(&self, opcode: u8, payload: &[u8]) {
        match self.stage.get() {
            Stage::Connecting | Stage::Handshaking => crate::panic_with(
                "InvalidStateError: Failed to execute 'send' on 'WebSocket': Still in CONNECTING \
                 state.",
            ),
            Stage::Open => self.queue_bytes(&client_frame(opcode, payload)),
            Stage::Closing | Stage::Closed => {}
        }
    }

    fn queue_bytes(&self, bytes: &[u8]) {
        self.outbox.borrow_mut().extend_from_slice(bytes);
    }

    /// Fails the connection (RFC 6455 §7.1.7): a Close frame with `status`
    /// when the socket is open enough to carry one, the TCP connection shut
    /// once it has gone out (at once, before the handshake), and `error` then
    /// `close` for the program.
    fn fail(&self, status: Option<u16>) {
        if self.stage.get() == Stage::Closed {
            return;
        }
        let was = self.stage.replace(Stage::Closed);
        self.connecting.borrow_mut().take();
        self.fragments.borrow_mut().take();
        match (was, status) {
            (Stage::Open | Stage::Closing, Some(status)) if !self.peer_gone.get() => {
                self.queue_bytes(&client_frame(OPCODE_CLOSE, &status.to_be_bytes()));
                self.shut_when_flushed.set(true);
            }
            _ => self.shut_now(),
        }
        let mut events = self.events.borrow_mut();
        events.push_back(Event::Error);
        events.push_back(Event::Close);
    }

    /// The closing handshake completed, or the peer went away during it: the
    /// connection is over cleanly as far as the program is concerned — `close`
    /// alone.
    fn finish_closed(&self) {
        if self.stage.replace(Stage::Closed) == Stage::Closed {
            return;
        }
        self.shut_when_flushed.set(true);
        self.events.borrow_mut().push_back(Event::Close);
    }

    fn shut_now(&self) {
        self.outbox.borrow_mut().clear();
        if let Some(stream) = self.stream.borrow_mut().take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }

    /// The helper thread's answer, when it has one.
    fn advance_connect(&self) -> bool {
        let answer = match self.connecting.borrow().as_ref().map(Receiver::try_recv) {
            None | Some(Err(TryRecvError::Empty)) => return false,
            Some(Ok(answer)) => Ok(answer),
            Some(Err(TryRecvError::Disconnected)) => Err(()),
        };
        self.connecting.borrow_mut().take();
        match answer {
            Ok(Ok(stream)) if stream.set_nonblocking(true).is_ok() => {
                let _ = stream.set_nodelay(true);
                *self.stream.borrow_mut() = Some(stream);
                let request = std::mem::take(&mut *self.request.borrow_mut());
                self.queue_bytes(&request);
                self.stage.set(Stage::Handshaking);
            }
            // Refused, unreachable, no such host: the host's `error` then
            // `close`, which is all `std::rpc` asks of a failed dial.
            _ => self.fail(None),
        }
        true
    }

    /// Writes what is queued and reads what has arrived. `true` if anything
    /// moved.
    fn advance_socket(&self) -> bool {
        let mut progressed = false;
        let mut dead = false;
        {
            let mut stream_slot = self.stream.borrow_mut();
            let Some(stream) = stream_slot.as_mut() else {
                return false;
            };
            let mut outbox = self.outbox.borrow_mut();
            while !outbox.is_empty() {
                match stream.write(&outbox) {
                    Ok(0) => {
                        dead = true;
                        break;
                    }
                    Ok(count) => {
                        outbox.drain(..count);
                        progressed = true;
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == ErrorKind::Interrupted => {}
                    Err(_) => {
                        dead = true;
                        break;
                    }
                }
            }
            if outbox.is_empty() && self.shut_when_flushed.get() {
                let _ = stream.shutdown(Shutdown::Both);
                stream_slot.take();
                return true;
            }
            if !self.peer_gone.get() && !dead {
                let mut chunk = [0u8; READ_CHUNK];
                loop {
                    match stream.read(&mut chunk) {
                        Ok(0) => {
                            dead = true;
                            break;
                        }
                        Ok(count) => {
                            self.inbox.borrow_mut().extend_from_slice(&chunk[..count]);
                            progressed = true;
                        }
                        Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                        Err(error) if error.kind() == ErrorKind::Interrupted => {}
                        // A reset is the peer going away, as EOF is.
                        Err(_) => {
                            dead = true;
                            break;
                        }
                    }
                }
            }
        }
        if dead && !self.peer_gone.replace(true) {
            progressed = true;
        }
        progressed
    }

    /// Reads the handshake answer once it is whole. `true` if the stage moved.
    fn advance_handshake(&self) -> bool {
        let end = head_end(&self.inbox.borrow());
        let Some(end) = end else {
            if self.peer_gone.get() || self.inbox.borrow().len() > MAX_HEAD {
                self.fail(None);
                return true;
            }
            return false;
        };
        let head: Vec<u8> = self.inbox.borrow_mut().drain(..end).collect();
        match verify_response(&head, &self.key, &self.protocols) {
            // What followed the head in the same read is frame data, and stays
            // in the inbox for the frame reader.
            Ok(()) => self.stage.set(Stage::Open),
            Err(_reason) => self.fail(None),
        }
        true
    }

    /// Reads frames until one makes an event for the program, or no whole
    /// frame is left. Control frames are answered here (a Ping's Pong, a
    /// Close's echo) and are not the program's.
    fn advance_frames(&self) {
        loop {
            if !matches!(self.stage.get(), Stage::Open | Stage::Closing) {
                return;
            }
            let taken = take_frame(&mut self.inbox.borrow_mut());
            let frame = match taken {
                Taken::More => {
                    if self.peer_gone.get() {
                        // The peer went away without a Close frame: abnormal
                        // (1006) — unless WE were closing, which it ended.
                        if self.stage.get() == Stage::Closing {
                            self.finish_closed();
                        } else {
                            self.fail(None);
                        }
                    }
                    return;
                }
                Taken::Fault(status) => {
                    self.fail(Some(status));
                    return;
                }
                Taken::Frame(frame) => frame,
            };
            match frame.opcode {
                OPCODE_PING => {
                    if self.stage.get() == Stage::Open {
                        self.queue_bytes(&client_frame(OPCODE_PONG, &frame.payload));
                    }
                }
                OPCODE_PONG => {}
                OPCODE_CLOSE => {
                    if frame.payload.len() == 1 {
                        self.fail(Some(STATUS_PROTOCOL_ERROR));
                        return;
                    }
                    if self.stage.get() == Stage::Open {
                        // Echo the status (§5.5.1), then close our end.
                        let echo = frame.payload.get(..2).unwrap_or_default().to_vec();
                        self.queue_bytes(&client_frame(OPCODE_CLOSE, &echo));
                    }
                    self.finish_closed();
                    return;
                }
                OPCODE_TEXT | OPCODE_BINARY => {
                    if self.fragments.borrow().is_some() {
                        // A new message inside a fragmented one.
                        self.fail(Some(STATUS_PROTOCOL_ERROR));
                        return;
                    }
                    if frame.fin {
                        if self.deliver_message(frame.opcode, frame.payload) {
                            return;
                        }
                    } else {
                        *self.fragments.borrow_mut() = Some((frame.opcode, frame.payload));
                    }
                }
                _ => {
                    // OPCODE_CONTINUATION — `take_frame` let no other through.
                    let Some((opcode, mut payload)) = self.fragments.borrow_mut().take() else {
                        self.fail(Some(STATUS_PROTOCOL_ERROR));
                        return;
                    };
                    payload.extend_from_slice(&frame.payload);
                    if frame.fin {
                        if self.deliver_message(opcode, payload) {
                            return;
                        }
                    } else {
                        *self.fragments.borrow_mut() = Some((opcode, payload));
                    }
                }
            }
        }
    }

    /// Queues one whole message for the program. `true` if it was queued; a
    /// text message that is not UTF-8 fails the connection (1007) instead.
    /// While closing, a message is discarded (the host delivers none once
    /// `close()` has been called).
    fn deliver_message(&self, opcode: u8, payload: Vec<u8>) -> bool {
        if self.stage.get() != Stage::Open {
            return false;
        }
        let message = if opcode == OPCODE_TEXT {
            match String::from_utf8(payload) {
                Ok(text) => SocketMessage::Text(str_new(&text)),
                Err(_) => {
                    self.fail(Some(STATUS_INVALID_PAYLOAD));
                    return true;
                }
            }
        } else {
            SocketMessage::Binary(Bytes::from_vec(payload))
        };
        self.events.borrow_mut().push_back(Event::Message(message));
        true
    }

    /// Hands ONE queued event to its handler, read at delivery time as the
    /// host reads `onmessage`/`onclose` when it fires.
    fn deliver_one(&self) -> bool {
        let Some(event) = self.events.borrow_mut().pop_front() else {
            return false;
        };
        // The handler is cloned out before the call: it may replace itself,
        // send, or close this very socket.
        match event {
            Event::Message(message) => {
                let handler = self.on_message.borrow().clone();
                if let Some(handler) = handler {
                    handler(message);
                }
            }
            Event::Error => {
                let handler = self.on_error.borrow().clone();
                if let Some(handler) = handler {
                    handler();
                }
            }
            Event::Close => {
                let handler = self.on_close.borrow().clone();
                if let Some(handler) = handler {
                    handler();
                }
            }
        }
        true
    }
}

impl IoSource for Client {
    fn poll(&self) -> bool {
        let mut progressed = false;
        if self.stage.get() == Stage::Connecting {
            progressed |= self.advance_connect();
        }
        progressed |= self.advance_socket();
        if self.stage.get() == Stage::Handshaking {
            progressed |= self.advance_handshake();
        }
        if self.stage.get() == Stage::Closing
            && self
                .close_deadline
                .get()
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.finish_closed();
            progressed = true;
        }
        if self.events.borrow().is_empty() {
            self.advance_frames();
        }
        progressed |= self.deliver_one();
        progressed
    }

    /// F160: the socket — read while the peer may still send, written while
    /// the outbox holds something — and a `close()`'s deadline. A connect in
    /// flight is the helper thread's, and it wakes the loop itself.
    fn interest(&self, interest: &mut Interest) -> bool {
        if let Some(stream) = self.stream.borrow().as_ref() {
            interest.watch(
                stream,
                !self.peer_gone.get(),
                !self.outbox.borrow().is_empty(),
            );
        }
        if self.stage.get() == Stage::Closing
            && let Some(deadline) = self.close_deadline.get()
        {
            interest.wake_at(deadline);
        }
        true
    }

    /// Live while it can still produce an event, or still has bytes to put on
    /// the wire before it shuts.
    fn is_live(&self) -> bool {
        self.stage.get() != Stage::Closed
            || !self.events.borrow().is_empty()
            || self.stream.borrow().is_some()
    }
}

impl PartialEq for WebSocket {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Js for WebSocket {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a `WebSocket` is a host object's own inspection, which the native backend \
             does not reproduce",
        )
    }
}

// --------------------------------------------------------------- message ---

/// `SocketMessage` — the host's `MessageEvent`, reduced to its `data`.
#[derive(Clone, Debug, PartialEq)]
pub enum SocketMessage {
    Text(Str),
    Binary(Bytes),
}

impl SocketMessage {
    /// `event.data` read as `str`. A binary message's `data` is an
    /// `ArrayBuffer`, which a string position stringifies as JavaScript does.
    pub fn data(&self) -> Str {
        match self {
            SocketMessage::Text(text) => Rc::clone(text),
            SocketMessage::Binary(_) => str_new("[object ArrayBuffer]"),
        }
    }

    /// `event.data` untyped — what `std::rpc` asks `host_kind` about.
    pub fn data_value(&self) -> JsonValue {
        match self {
            SocketMessage::Text(text) => JsonValue::Text(Rc::clone(text)),
            SocketMessage::Binary(bytes) => JsonValue::Buffer(bytes.clone()),
        }
    }

    /// `event.data` as the raw buffer — valid only for a binary message, as on
    /// the host; a text message answers its UTF-8 bytes.
    pub fn data_buffer(&self) -> Bytes {
        match self {
            SocketMessage::Text(text) => Bytes::from_vec(text.as_bytes().to_vec()),
            SocketMessage::Binary(bytes) => bytes.clone(),
        }
    }
}

impl Js for SocketMessage {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a `MessageEvent` is a host object's own inspection, which the native \
             backend does not reproduce",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::{block_on, nursery_new, on_abort, promise, sleep};
    use crate::http::{Handler, Request, Socket, create_server};
    use std::net::TcpListener;

    // -- the URL, per rule --

    #[test]
    fn a_ws_url_splits_into_host_port_authority_and_resource() {
        assert_eq!(
            parse_url("ws://Example.com:4000/a/b?c=d"),
            Ok(Target {
                host: "example.com".to_string(),
                port: 4000,
                authority: "example.com:4000".to_string(),
                resource: "/a/b?c=d".to_string(),
            })
        );
        // The default port is left out of `Host`, and the resource is at
        // least `/` — a bare query included.
        let bare = parse_url("ws://localhost").expect("a bare host");
        assert_eq!((bare.port, bare.authority.as_str()), (80, "localhost"));
        assert_eq!(bare.resource, "/");
        assert_eq!(
            parse_url("ws://localhost?x=1").expect("a query").resource,
            "/?x=1"
        );
        // `http:` is the host's alias for `ws:`.
        assert_eq!(parse_url("http://a:1/").expect("http").port, 1);
    }

    #[test]
    fn an_ipv6_literal_keeps_its_brackets_in_host_and_drops_them_to_connect() {
        let target = parse_url("ws://[::1]:9000/x").expect("an IPv6 literal");
        assert_eq!(target.host, "::1");
        assert_eq!(target.authority, "[::1]:9000");
        assert_eq!(parse_url("ws://[::1]/").expect("no port").port, 80);
    }

    #[test]
    fn tls_is_refused_by_name_and_a_malformed_url_by_reason() {
        assert_eq!(parse_url("wss://a/"), Err(UrlError::Tls));
        assert_eq!(parse_url("HTTPS://a/"), Err(UrlError::Tls));
        for bad in [
            "a/b",
            "ftp://a/",
            "ws:///x",
            "ws://a/#frag",
            "ws://u:p@a/",
            "ws://a:0/",
            "ws://a:70000/",
            "ws://[::1/",
        ] {
            assert!(
                matches!(parse_url(bad), Err(UrlError::Invalid(_))),
                "{bad} must be refused"
            );
        }
    }

    #[test]
    fn dialling_wss_panics_with_the_tls_refusal() {
        let refused = std::panic::catch_unwind(|| WebSocket::connect("wss://example.com/", &[]))
            .err()
            .expect("wss must be refused");
        let message = refused
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| refused.downcast_ref::<&str>().map(|text| text.to_string()))
            .unwrap_or_default();
        assert!(
            message.contains("no TLS") && message.contains("wss://example.com/"),
            "{message}"
        );
    }

    // -- the handshake, per rule --

    #[test]
    fn the_accept_key_is_rfc_6455s_worked_example() {
        assert_eq!(
            accept_for("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn the_upgrade_request_offers_the_key_the_version_and_the_protocols() {
        let target = parse_url("ws://a:81/p").expect("a url");
        let protocols = [str_new("vilan-rpc"), str_new("token.t")];
        let request = String::from_utf8(upgrade_request(&target, "KEY", &protocols)).expect("text");
        assert_eq!(
            request,
            "GET /p HTTP/1.1\r\nHost: a:81\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: KEY\r\nSec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Protocol: vilan-rpc, token.t\r\n\r\n"
        );
        let bare = String::from_utf8(upgrade_request(&target, "KEY", &[])).expect("text");
        assert!(!bare.contains("Sec-WebSocket-Protocol"), "{bare}");
    }

    fn response(status: &str, extra: &str, key: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 {status}\r\nUpgrade: WebSocket\r\nConnection: keep-alive, Upgrade\r\n\
             Sec-WebSocket-Accept: {}\r\n{extra}\r\n",
            accept_for(key)
        )
        .into_bytes()
    }

    #[test]
    fn a_handshake_answer_is_held_to_every_client_requirement() {
        let offered = [str_new("vilan-rpc")];
        assert_eq!(
            verify_response(&response("101 Switching Protocols", "", "k"), "k", &offered),
            Ok(())
        );
        assert_eq!(
            verify_response(
                &response(
                    "101 Switching Protocols",
                    "Sec-WebSocket-Protocol: vilan-rpc\r\n",
                    "k"
                ),
                "k",
                &offered
            ),
            Ok(())
        );
        for (head, why) in [
            (response("200 OK", "", "k"), "not a 101"),
            (
                response("101 Switching Protocols", "", "other"),
                "a wrong accept",
            ),
            (
                response(
                    "101 Switching Protocols",
                    "Sec-WebSocket-Protocol: chat\r\n",
                    "k",
                ),
                "an unoffered protocol",
            ),
            (
                response(
                    "101 Switching Protocols",
                    "Sec-WebSocket-Extensions: permessage-deflate\r\n",
                    "k",
                ),
                "an unoffered extension",
            ),
            (
                b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\n\r\n".to_vec(),
                "no Upgrade header",
            ),
        ] {
            assert!(verify_response(&head, "k", &offered).is_err(), "{why}");
        }
    }

    // -- frames, per rule --

    /// A server frame: unmasked, FIN as given.
    fn server_frame(fin: bool, opcode: u8, payload: &[u8]) -> Vec<u8> {
        let mut frame = vec![if fin { 0x80 } else { 0 } | opcode];
        if payload.len() < 126 {
            frame.push(payload.len() as u8);
        } else if payload.len() <= 0xFFFF {
            frame.push(126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        } else {
            frame.push(127);
            frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }
        frame.extend_from_slice(payload);
        frame
    }

    /// Takes one CLIENT frame off `buffer`, unmasking it — what a server reads.
    fn read_client_frame(buffer: &mut Vec<u8>) -> Option<(u8, Vec<u8>)> {
        if buffer.len() < 2 {
            return None;
        }
        assert_eq!(buffer[1] & 0x80, 0x80, "every client frame is masked");
        let (length, mut offset) = match buffer[1] & 0x7F {
            126 => (u16::from_be_bytes([buffer[2], buffer[3]]) as usize, 4),
            127 => {
                let mut wide = [0u8; 8];
                wide.copy_from_slice(&buffer[2..10]);
                (u64::from_be_bytes(wide) as usize, 10)
            }
            short => (short as usize, 2),
        };
        if buffer.len() < offset + 4 + length {
            return None;
        }
        let mask = [
            buffer[offset],
            buffer[offset + 1],
            buffer[offset + 2],
            buffer[offset + 3],
        ];
        offset += 4;
        let payload = buffer[offset..offset + length]
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4])
            .collect();
        let opcode = buffer[0] & 0x0F;
        buffer.drain(..offset + length);
        Some((opcode, payload))
    }

    #[test]
    fn a_client_frame_is_masked_at_each_length_form() {
        for size in [0usize, 5, 125, 126, 65_535, 65_536] {
            let payload: Vec<u8> = (0..size).map(|index| index as u8).collect();
            let mut frame = client_frame(OPCODE_BINARY, &payload);
            assert_eq!(frame[0], 0x80 | OPCODE_BINARY);
            assert_eq!(
                read_client_frame(&mut frame),
                Some((OPCODE_BINARY, payload)),
                "{size}"
            );
            assert!(frame.is_empty());
        }
    }

    #[test]
    fn two_masks_differ() {
        let first = client_frame(OPCODE_TEXT, b"x");
        let second = client_frame(OPCODE_TEXT, b"x");
        assert_ne!(
            first[2..6],
            second[2..6],
            "the masking key is fresh per frame"
        );
    }

    #[test]
    fn a_server_frame_is_taken_whole_or_not_at_all() {
        let whole = server_frame(true, OPCODE_TEXT, &[b'a'; 300]);
        for cut in [1, 2, 3, 100, whole.len() - 1] {
            let mut partial = whole[..cut].to_vec();
            assert_eq!(take_frame(&mut partial), Taken::More, "{cut}");
            assert_eq!(
                partial.len(),
                cut,
                "nothing is consumed from a partial frame"
            );
        }
        let mut inbox = whole.clone();
        inbox.extend_from_slice(&server_frame(true, OPCODE_PING, b"p"));
        assert_eq!(
            take_frame(&mut inbox),
            Taken::Frame(Frame {
                fin: true,
                opcode: OPCODE_TEXT,
                payload: vec![b'a'; 300]
            })
        );
        assert_eq!(
            take_frame(&mut inbox),
            Taken::Frame(Frame {
                fin: true,
                opcode: OPCODE_PING,
                payload: b"p".to_vec()
            })
        );
        assert!(inbox.is_empty());
    }

    #[test]
    fn a_frame_no_server_may_send_is_a_protocol_error() {
        let mut masked = server_frame(true, OPCODE_TEXT, b"x");
        masked[1] |= 0x80;
        let mut reserved = server_frame(true, OPCODE_TEXT, b"x");
        reserved[0] |= 0x40;
        let mut unknown = server_frame(true, 0x3, b"x");
        let mut fragmented_control = server_frame(false, OPCODE_PING, b"x");
        let mut long_control = server_frame(true, OPCODE_PING, &[0; 126]);
        for (inbox, why) in [
            (&mut masked, "a masked server frame"),
            (&mut reserved, "a reserved bit"),
            (&mut unknown, "an unknown opcode"),
            (&mut fragmented_control, "a fragmented control frame"),
            (&mut long_control, "a control frame over 125 bytes"),
        ] {
            assert_eq!(
                take_frame(inbox),
                Taken::Fault(STATUS_PROTOCOL_ERROR),
                "{why}"
            );
        }
    }

    // -- the executor's host one-shots --

    #[test]
    fn an_abort_hook_runs_at_the_cancel_at_once_when_already_aborted_and_never_once_detached() {
        let fired = Rc::new(RefCell::new(Vec::new()));
        let nursery = nursery_new(None);
        let signal = nursery.signal();
        let record = |label: &'static str| {
            let fired = Rc::clone(&fired);
            Rc::new(move || fired.borrow_mut().push(label)) as Rc<dyn Fn()>
        };
        let _kept = on_abort(&signal, record("kept"));
        let detached = on_abort(&signal, record("detached"));
        detached.detach();
        assert!(fired.borrow().is_empty(), "nothing runs before the cancel");
        nursery.cancel();
        nursery.cancel();
        assert_eq!(
            *fired.borrow(),
            ["kept"],
            "once, at the cancel, detached excluded"
        );
        let _late = on_abort(&signal, record("late"));
        assert_eq!(
            *fired.borrow(),
            ["kept", "late"],
            "an aborted signal runs at once"
        );
        // A child nursery's hook runs when the PARENT cancels.
        let parent = nursery_new(None);
        let child = nursery_new(Some(parent.clone()));
        let _child_hook = on_abort(&child.signal(), record("child"));
        parent.cancel();
        assert_eq!(*fired.borrow(), ["kept", "late", "child"]);
    }

    #[test]
    fn a_promise_settles_on_its_first_resolve_and_ignores_the_rest() {
        let settled = block_on(async {
            let early = promise(Rc::new(|resolve: Rc<dyn Fn(i32)>| {
                resolve(1);
                resolve(2);
            }));
            let resolver: Rc<RefCell<Option<Rc<dyn Fn(i32)>>>> = Rc::new(RefCell::new(None));
            let held = Rc::clone(&resolver);
            let late = promise(Rc::new(move |resolve: Rc<dyn Fn(i32)>| {
                *held.borrow_mut() = Some(resolve);
            }));
            assert!(!late.is_settled(), "an unresolved promise waits");
            let resolve = resolver.borrow().clone().expect("the executor ran");
            resolve(3);
            resolve(4);
            (early.await, late.await)
        });
        assert_eq!(settled, (1, 3));
    }

    // -- loopback against the native server's own upgrade path --

    /// What a test socket saw, in order.
    #[derive(Default)]
    struct Seen {
        events: RefCell<Vec<String>>,
    }

    impl Seen {
        fn push(&self, event: String) {
            self.events.borrow_mut().push(event);
        }
    }

    /// Wires every handler of `socket` into `seen`, and answers a task that
    /// settles at the close event.
    fn watch(socket: &WebSocket, seen: &Rc<Seen>) -> crate::executor::Task<()> {
        let on_message = Rc::clone(seen);
        socket.set_on_message(Rc::new(move |message: SocketMessage| {
            on_message.push(match message {
                SocketMessage::Text(text) => format!("text:{text}"),
                SocketMessage::Binary(bytes) => format!("binary:{:?}", &*bytes.as_slice()),
            });
        }));
        let on_error = Rc::clone(seen);
        socket.set_on_error(Rc::new(move || on_error.push("error".to_string())));
        let on_close = Rc::clone(seen);
        let socket = socket.clone();
        promise(Rc::new(move |resolve: Rc<dyn Fn(())>| {
            let on_close = Rc::clone(&on_close);
            socket.set_on_close(Rc::new(move || {
                on_close.push("close".to_string());
                resolve(());
            }));
        }))
    }

    /// A native `Server` whose upgrade handler performs the server half of the
    /// handshake (selecting the first offered protocol) and then runs `serve`
    /// on every whole client frame. Answers the server and its port.
    fn upgrading_server(serve: Rc<dyn Fn(&Socket, u8, Vec<u8>)>) -> (crate::http::Server, u16) {
        let handler: Handler = Rc::new(|_request: Request, response: crate::http::Response| {
            crate::executor::pin_future(async move { response.end("no") })
        });
        let server = create_server(handler);
        server.on_upgrade(
            "upgrade",
            Rc::new(move |request: Request, socket: Socket, _head: Bytes| {
                let key = request.header("sec-websocket-key").unwrap_or_default();
                let protocol = request
                    .header("sec-websocket-protocol")
                    .map(|offered| {
                        let first = offered
                            .split(',')
                            .next()
                            .unwrap_or_default()
                            .trim()
                            .to_string();
                        format!("Sec-WebSocket-Protocol: {first}\r\n")
                    })
                    .unwrap_or_default();
                socket.write_text(&format!(
                    "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\
                     Connection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n{protocol}\r\n",
                    accept_for(&key)
                ));
                let buffer = Rc::new(RefCell::new(Vec::new()));
                let serve = Rc::clone(&serve);
                let replying = socket.clone();
                socket.on_bytes(
                    "data",
                    Rc::new(move |bytes: Bytes| {
                        buffer.borrow_mut().extend_from_slice(&bytes.as_slice());
                        loop {
                            let next = read_client_frame(&mut buffer.borrow_mut());
                            let Some((opcode, payload)) = next else {
                                break;
                            };
                            serve(&replying, opcode, payload);
                        }
                    }),
                );
            }),
        );
        server.listen(0, Rc::new(|| {}));
        let port = server.port() as u16;
        (server, port)
    }

    fn write_frame(socket: &Socket, fin: bool, opcode: u8, payload: &[u8]) {
        socket.write_bytes(&Bytes::from_vec(server_frame(fin, opcode, payload)));
    }

    #[test]
    fn text_and_binary_echo_through_the_native_server_and_close_cleanly() {
        let seen = Rc::new(Seen::default());
        let server_saw = Rc::new(Seen::default());
        let outcome = Rc::clone(&seen);
        let observed = Rc::clone(&server_saw);
        block_on(async move {
            let (server, port) = upgrading_server(Rc::new(move |socket, opcode, payload| {
                observed.push(format!("{opcode}:{}", String::from_utf8_lossy(&payload)));
                match opcode {
                    OPCODE_TEXT => {
                        // A fragmented answer: the client reassembles it.
                        write_frame(socket, false, OPCODE_TEXT, b"echo:");
                        write_frame(socket, true, OPCODE_CONTINUATION, &payload);
                    }
                    OPCODE_BINARY => write_frame(socket, true, OPCODE_BINARY, &payload),
                    OPCODE_CLOSE => {
                        write_frame(socket, true, OPCODE_CLOSE, &payload);
                        socket.destroy();
                    }
                    _ => {}
                }
            }));
            let socket = WebSocket::connect(
                &format!("ws://127.0.0.1:{port}/rpc"),
                &[str_new("vilan-rpc")],
            );
            let closed = watch(&socket, &outcome);
            // The first message is sent once the socket is open — polled for,
            // since `std::rpc` learns that from the server's own first frame.
            while socket.0.stage.get() != Stage::Open {
                sleep(1, None).await;
            }
            socket.transmit("hello");
            socket.transmit_bytes(&Bytes::from_vec(vec![1, 2, 3]));
            while outcome.events.borrow().len() < 2 {
                sleep(1, None).await;
            }
            socket.close();
            closed.await;
            server.close(Rc::new(|| {}));
        });
        assert_eq!(
            *seen.events.borrow(),
            ["text:echo:hello", "binary:[1, 2, 3]", "close"]
        );
        assert_eq!(
            *server_saw.events.borrow(),
            ["1:hello", "2:\u{1}\u{2}\u{3}", "8:"]
        );
    }

    #[test]
    fn a_ping_is_answered_and_a_server_close_is_echoed() {
        let seen = Rc::new(Seen::default());
        let server_saw = Rc::new(Seen::default());
        let outcome = Rc::clone(&seen);
        let observed = Rc::clone(&server_saw);
        block_on(async move {
            let (server, port) = upgrading_server(Rc::new(move |socket, opcode, payload| {
                observed.push(format!("{opcode}:{payload:?}"));
                if opcode == OPCODE_TEXT {
                    write_frame(socket, true, OPCODE_PING, b"are you there");
                } else if opcode == OPCODE_PONG {
                    // The pong arrived: close, status 4000 and a reason.
                    let mut close = 4000u16.to_be_bytes().to_vec();
                    close.extend_from_slice(b"bye");
                    write_frame(socket, true, OPCODE_CLOSE, &close);
                }
            }));
            let socket = WebSocket::connect(&format!("ws://127.0.0.1:{port}/"), &[]);
            let closed = watch(&socket, &outcome);
            while socket.0.stage.get() != Stage::Open {
                sleep(1, None).await;
            }
            socket.transmit("start");
            closed.await;
            // The echoed Close is in flight; give the server a beat to read it.
            sleep(50, None).await;
            server.close(Rc::new(|| {}));
        });
        assert_eq!(
            *seen.events.borrow(),
            ["close"],
            "a clean close is no error"
        );
        let pong = format!("{OPCODE_PONG}:{:?}", b"are you there".to_vec());
        let echo = format!("{OPCODE_CLOSE}:{:?}", 4000u16.to_be_bytes().to_vec());
        assert_eq!(
            *server_saw.events.borrow(),
            [format!("{OPCODE_TEXT}:{:?}", b"start".to_vec()), pong, echo]
        );
    }

    #[test]
    fn a_refused_connection_is_an_error_then_a_close() {
        // A port that was just free: bound, read, released.
        let port = TcpListener::bind("127.0.0.1:0")
            .expect("bind")
            .local_addr()
            .expect("address")
            .port();
        let seen = Rc::new(Seen::default());
        let outcome = Rc::clone(&seen);
        block_on(async move {
            let socket = WebSocket::connect(&format!("ws://127.0.0.1:{port}/"), &[]);
            watch(&socket, &outcome).await;
        });
        assert_eq!(*seen.events.borrow(), ["error", "close"]);
    }

    #[test]
    fn a_server_that_will_not_upgrade_fails_the_handshake() {
        let seen = Rc::new(Seen::default());
        let outcome = Rc::clone(&seen);
        block_on(async move {
            let handler: Handler = Rc::new(|_request: Request, response: crate::http::Response| {
                crate::executor::pin_future(async move {
                    response.set_status_code(404);
                    response.end("nope");
                })
            });
            // No upgrade handler: the native server answers the upgrade as a
            // plain request.
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port();
            let socket = WebSocket::connect(&format!("ws://127.0.0.1:{port}/"), &[]);
            watch(&socket, &outcome).await;
            server.close(Rc::new(|| {}));
        });
        assert_eq!(*seen.events.borrow(), ["error", "close"]);
    }

    #[test]
    fn closing_a_hung_dial_shuts_the_half_open_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        // The harness's side: accept, read the upgrade request, then read on
        // until the client's connection goes away.
        let peer = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(30)))
                .expect("bound the read");
            let mut received = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => return String::from_utf8_lossy(&received).to_string(),
                    Ok(count) => received.extend_from_slice(&chunk[..count]),
                    Err(error) if error.kind() == ErrorKind::ConnectionReset => {
                        return String::from_utf8_lossy(&received).to_string();
                    }
                    Err(error) => panic!("the connection was never closed: {error}"),
                }
            }
        });
        let seen = Rc::new(Seen::default());
        let outcome = Rc::clone(&seen);
        block_on(async move {
            let socket = WebSocket::connect(&format!("ws://127.0.0.1:{port}/hung"), &[]);
            let closed = watch(&socket, &outcome);
            while socket.0.stage.get() == Stage::Connecting || !socket.0.outbox.borrow().is_empty()
            {
                sleep(1, None).await;
            }
            socket.close();
            closed.await;
        });
        let request = peer.join().expect("the peer thread");
        assert!(request.starts_with("GET /hung HTTP/1.1\r\n"), "{request:?}");
        assert_eq!(*seen.events.borrow(), ["error", "close"]);
    }

    #[test]
    fn sending_before_the_socket_opens_throws_as_the_host_does() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        let socket = WebSocket::connect(&format!("ws://127.0.0.1:{port}/"), &[]);
        let thrown = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            socket.transmit("too early");
        }));
        assert!(thrown.is_err(), "a CONNECTING socket refuses a send");
        socket.close();
    }
}
