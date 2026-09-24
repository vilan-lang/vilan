//! `vilan-rt::http` — an HTTP/1.1 server over `std::net`, by hand (tracker F18
//! slice 1; `proposal/native-apps.md` §11 sizes it, Order 39's R1 rules it).
//!
//! # Why by hand
//!
//! R1: `vilan-rt` takes no crates.io dependencies. That is Order 37's R8 and it
//! is what lets a `--backend rust` build work from an installed toolchain with
//! nothing but a Rust toolchain on the machine. HTTP/1.1 over TCP is a
//! text protocol with a length-delimited body and it fits inside that rule; SQLite
//! does not, which is why `std::db` natively is a separate crate and Order 40's.
//!
//! # What this is a twin OF
//!
//! `vilan/std/src/process/http.vl` binds `node:http` — `createServer`, the
//! request's `url`/`method`/`headers`, the response's `statusCode`/`setHeader`/
//! `write`/`end`, the server's `listen`/`address`/`close`, and the raw socket a
//! protocol upgrade hands over. Those bindings ARE the contract: every item here
//! exists because one of them does, and the comment at each one names it. Where
//! node's behaviour and this one genuinely cannot agree the divergence is
//! written down at the site (see [`ResponseBody::end`] on the response header
//! set, which is the one place a byte comparison against node is impossible —
//! node sends a `Date`).
//!
//! # How it runs
//!
//! Every socket is non-blocking and every socket is an
//! [`crate::executor::IoSource`], polled once per executor turn. The listener
//! accepts; a connection reads until it has a whole request; the request
//! handler is SPAWNED, exactly as node treats the promise its callback returns
//! (floating — an unhandled rejection reports and the server keeps serving); the
//! response's bytes go into an outbox the same poll flushes. Nothing blocks the
//! loop, so a program can serve requests and run timers at once.
//!
//! # Not in v1, by name
//!
//! Keep-alive (every response says `Connection: close`), `Transfer-Encoding:
//! chunked` on the way IN (a request that sends one is refused with `411`),
//! HTTP/2, TLS, and the WebSocket handshake itself — the upgrade HANDOVER is
//! here, so a `std::ws` written in vilan has a socket to write its handshake to,
//! but nothing here computes an RFC 6455 accept key.

use std::cell::{Cell, RefCell};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::bytes::Bytes;
use crate::executor::{Boxed, IoSource, register_io, spawn};
use crate::json::JsonValue;
use crate::{Js, Str, str_new};

/// One read of a connection. 8 KiB is node's own default highWaterMark for a
/// socket, which is the only reason to prefer it to any other number.
const READ_CHUNK: usize = 8 * 1024;

/// The largest request head this accepts before answering `431`. A head is
/// buffered whole before it can be parsed, so an unbounded one is an unbounded
/// allocation driven by a stranger.
const MAX_HEAD: usize = 64 * 1024;

/// The largest body this accepts before answering `413`, for the same reason.
const MAX_BODY: usize = 16 * 1024 * 1024;

// ---------------------------------------------------------------- parsing ---

/// A request head, parsed off the wire.
///
/// Field names are lowercased on the way in, because node lowercases every name
/// it parses and `std::http`'s `Request::header` is documented against that: it
/// lowers the name it is asked for and expects to find it.
#[derive(Debug, PartialEq)]
struct Head {
    method: String,
    target: String,
    /// `(lowercased name, value)`, in wire order. A repeated field is JOINED
    /// with `", "`, which is node's own presentation and what `Request::header`
    /// documents — except `set-cookie`, which node keeps as a list and which
    /// therefore joins with a bare comma there. A request carrying `set-cookie`
    /// is degenerate, and this matches the documented behaviour rather than
    /// inventing a third one.
    fields: Vec<(String, String)>,
    /// How many bytes of the head were consumed, so the body starts there.
    length: usize,
}

/// Why a head could not be read.
#[derive(Debug, PartialEq)]
enum HeadError {
    /// Not all of it has arrived. Keep reading.
    Incomplete,
    /// It will never be a request. The status to answer with, and the reason.
    Refused(u16, &'static str),
}

/// Finds the end of the head (`CRLF CRLF`, or a bare `LF LF` — RFC 9112 §2.2
/// says a recipient MAY accept a bare `LF` as a line terminator, and node does).
fn head_end(buffer: &[u8]) -> Option<usize> {
    let mut index = 0;
    while index < buffer.len() {
        if buffer[index..].starts_with(b"\r\n\r\n") {
            return Some(index + 4);
        }
        if buffer[index..].starts_with(b"\n\n") {
            return Some(index + 2);
        }
        index += 1;
    }
    None
}

/// Parses one request head out of `buffer`.
///
/// Three refusals rather than a panic, because every one of them is something a
/// stranger on a socket can send: a head bigger than [`MAX_HEAD`], a line that
/// is not a request line, and a `Content-Length` that is not a number.
fn parse_head(buffer: &[u8]) -> Result<Head, HeadError> {
    let Some(length) = head_end(buffer) else {
        if buffer.len() > MAX_HEAD {
            return Err(HeadError::Refused(431, "Request Header Fields Too Large"));
        }
        return Err(HeadError::Incomplete);
    };
    // The head is ASCII by RFC 9110; anything else is not a head this will
    // answer. Lossy decoding would invent characters, so a non-UTF-8 head is
    // refused rather than repaired.
    let Ok(text) = std::str::from_utf8(&buffer[..length]) else {
        return Err(HeadError::Refused(400, "Bad Request"));
    };
    let mut lines = text.split('\n').map(|line| line.trim_end_matches('\r'));
    let Some(request_line) = lines.next() else {
        return Err(HeadError::Refused(400, "Bad Request"));
    };
    let mut parts = request_line.split(' ').filter(|part| !part.is_empty());
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Err(HeadError::Refused(400, "Bad Request"));
    };
    // The version is read and not enforced: a `HTTP/1.0` request is answered on
    // the same terms, since every response here already says `Connection:
    // close`, which is 1.0's own default.
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(HeadError::Refused(400, "Bad Request"));
        };
        // A field name may not be followed by whitespace before the colon (RFC
        // 9112 §5.1 makes that a must-reject, because a proxy that trims it
        // and one that does not disagree about the message).
        if name.ends_with(' ') || name.ends_with('\t') {
            return Err(HeadError::Refused(400, "Bad Request"));
        }
        let name = name.to_ascii_lowercase();
        let value = value.trim().to_string();
        match fields.iter_mut().find(|(known, _)| *known == name) {
            Some((_, existing)) => {
                let separator = if name == "set-cookie" { "," } else { ", " };
                existing.push_str(separator);
                existing.push_str(&value);
            }
            None => fields.push((name, value)),
        }
    }
    Ok(Head {
        method: method.to_string(),
        target: target.to_string(),
        fields,
        length,
    })
}

/// The declared body length, or a refusal.
///
/// A `Transfer-Encoding` is refused rather than half-handled: this server does
/// not decode chunked requests in v1, and reading a chunked body as if it were
/// length-delimited would hand the handler framing bytes as content.
fn body_length(head: &Head) -> Result<usize, HeadError> {
    if head
        .fields
        .iter()
        .any(|(name, _)| name == "transfer-encoding")
    {
        return Err(HeadError::Refused(411, "Length Required"));
    }
    let Some((_, value)) = head
        .fields
        .iter()
        .find(|(name, _)| name == "content-length")
    else {
        return Ok(0);
    };
    let Ok(length) = value.trim().parse::<usize>() else {
        return Err(HeadError::Refused(400, "Bad Request"));
    };
    if length > MAX_BODY {
        return Err(HeadError::Refused(413, "Content Too Large"));
    }
    Ok(length)
}

/// Whether the head asks for a protocol upgrade (`node:http`'s `upgrade` event,
/// which fires on `Connection: upgrade` with an `Upgrade` field).
fn asks_to_upgrade(head: &Head) -> bool {
    let has = |wanted: &str| {
        head.fields
            .iter()
            .find(|(name, _)| name == wanted)
            .map(|(_, value)| value.to_ascii_lowercase())
    };
    has("upgrade").is_some()
        && has("connection")
            .is_some_and(|value| value.split(',').any(|token| token.trim() == "upgrade"))
}

/// The reason phrase for a status code.
///
/// Only the codes this server MINTS itself need one; a code the application
/// chose gets the phrase RFC 9110 registers for it where there is one, and an
/// empty phrase otherwise — which is legal (the phrase is advisory and may be
/// empty) and is better than inventing text for a code the language does not
/// know.
fn reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Content",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "",
    }
}

// ------------------------------------------------------------- connection ---

/// What a connection is doing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
    /// Reading a request head and its body.
    Reading,
    /// The handler has the request; the response may still be open (a stream).
    Dispatched,
    /// The response ended; flush what is left and close.
    Closing,
    /// Handed over to an upgrade handler: reads go to its `on_bytes`.
    Upgraded,
    /// Gone.
    Closed,
}

/// One accepted connection: the socket, what has been read, what is waiting to
/// be written, and the stage.
struct Connection {
    stream: RefCell<TcpStream>,
    peer: String,
    inbox: RefCell<Vec<u8>>,
    outbox: RefCell<Vec<u8>>,
    stage: Cell<Stage>,
    /// An upgraded socket's subscribers (`socket.on("data" | "close" | …)`).
    on_bytes: RefCell<Vec<Rc<dyn Fn(Bytes)>>>,
    on_close: RefCell<Vec<Rc<dyn Fn()>>>,
    on_timeout: RefCell<Vec<Rc<dyn Fn()>>>,
    /// `socket.setTimeout(ms)`: fire `"timeout"` after this much inactivity.
    /// Zero clears it, as node's does.
    idle_limit: Cell<Option<Duration>>,
    last_activity: Cell<Instant>,
    /// Whether the idle timeout has already fired for the current quiet period.
    /// Node re-arms on activity, so this clears whenever anything moves.
    timed_out: Cell<bool>,
    /// `socket.destroyed`. It flips at the `destroy()` CALL, where node's does,
    /// even though the socket is closed one poll later once the outbox has been
    /// flushed — see [`Connection::destroy`].
    destroyed: Cell<bool>,
}

impl Connection {
    fn new(stream: TcpStream, peer: String) -> Connection {
        Connection {
            stream: RefCell::new(stream),
            peer,
            inbox: RefCell::new(Vec::new()),
            outbox: RefCell::new(Vec::new()),
            stage: Cell::new(Stage::Reading),
            on_bytes: RefCell::new(Vec::new()),
            on_close: RefCell::new(Vec::new()),
            on_timeout: RefCell::new(Vec::new()),
            idle_limit: Cell::new(None),
            last_activity: Cell::new(Instant::now()),
            timed_out: Cell::new(false),
            destroyed: Cell::new(false),
        }
    }

    fn touch(&self) {
        self.last_activity.set(Instant::now());
        self.timed_out.set(false);
    }

    /// Reads whatever has arrived. Answers `(progressed, the peer hung up)`.
    fn read_available(&self) -> (bool, bool) {
        let mut progressed = false;
        let mut chunk = [0u8; READ_CHUNK];
        loop {
            let read = self.stream.borrow_mut().read(&mut chunk);
            match read {
                Ok(0) => return (progressed, true),
                Ok(count) => {
                    self.inbox.borrow_mut().extend_from_slice(&chunk[..count]);
                    progressed = true;
                    self.touch();
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    return (progressed, false);
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                // Any other error is a dead connection. A reset by the peer is
                // the common one and is not an event the program hears about,
                // exactly as node drops it.
                Err(_) => return (progressed, true),
            }
        }
    }

    /// Pushes whatever is queued. Answers `(progressed, the socket is dead)`.
    fn write_available(&self) -> (bool, bool) {
        let mut progressed = false;
        loop {
            let queued = self.outbox.borrow().len();
            if queued == 0 {
                return (progressed, false);
            }
            let written = {
                let pending = self.outbox.borrow();
                self.stream.borrow_mut().write(&pending)
            };
            match written {
                Ok(0) => return (progressed, true),
                Ok(count) => {
                    self.outbox.borrow_mut().drain(..count);
                    progressed = true;
                    self.touch();
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    return (progressed, false);
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => return (progressed, true),
            }
        }
    }

    fn enqueue(&self, bytes: &[u8]) {
        if self.destroyed.get() || self.stage.get() == Stage::Closed {
            // Node silently drops a write to a destroyed socket; so does this,
            // and `std::http` documents it that way.
            return;
        }
        self.outbox.borrow_mut().extend_from_slice(bytes);
    }

    /// `socket.destroy()` / the end of a response: no more writes are accepted
    /// and the socket closes once what is already queued has gone out.
    ///
    /// The flush is why this is not [`Connection::close`]. A handler that writes
    /// a WebSocket handshake and then destroys the socket — which is exactly
    /// what a rejected upgrade looks like — has its bytes in the outbox and not
    /// in the kernel, because every write here is non-blocking and deferred to
    /// the poll. Closing at the call would discard them, so `destroyed` flips
    /// immediately (node's does) and the SHUTDOWN waits one poll.
    fn destroy(&self) {
        if self.stage.get() == Stage::Closed {
            return;
        }
        self.destroyed.set(true);
        self.stage.set(Stage::Closing);
    }

    fn close(&self) {
        if self.stage.get() == Stage::Closed {
            return;
        }
        self.stage.set(Stage::Closed);
        let _ = self.stream.borrow().shutdown(Shutdown::Both);
        for listener in self.on_close.borrow().iter() {
            listener();
        }
    }
}

// ----------------------------------------------------------------- socket ---

/// `NodeSocket` — the raw TCP socket a protocol upgrade hands over.
#[derive(Clone)]
pub struct Socket(Rc<Connection>);

impl Socket {
    /// `socket.write(text)`.
    pub fn write_text(&self, data: &str) {
        self.0.enqueue(data.as_bytes());
    }

    /// `socket.write(bytes)`.
    pub fn write_bytes(&self, data: &Bytes) {
        self.0.enqueue(&data.as_slice());
    }

    /// `socket.on("data", handler)`.
    pub fn on_bytes(&self, event: &str, handler: Rc<dyn Fn(Bytes)>) {
        if event == "data" {
            self.0.on_bytes.borrow_mut().push(handler);
        }
    }

    /// `socket.on("close" | "error" | "timeout", handler)`.
    ///
    /// `"error"` is accepted and never fires: a socket error here closes the
    /// connection, and node's own `error` event on a raw upgrade socket is
    /// followed by a `close` anyway, so a subscriber that only wants to know
    /// the socket is gone hears it once rather than twice.
    pub fn on_signal(&self, event: &str, handler: Rc<dyn Fn()>) {
        match event {
            "close" | "error" => self.0.on_close.borrow_mut().push(handler),
            "timeout" => self.0.on_timeout.borrow_mut().push(handler),
            _ => {}
        }
    }

    /// `socket.setTimeout(millis)` — fire `"timeout"` after that much
    /// inactivity; `0` clears it. It does NOT close the socket, which is the
    /// property `std::rpc_server` arms a greeting bound with.
    pub fn set_timeout(&self, millis: i32) {
        self.0.touch();
        self.0.idle_limit.set(if millis <= 0 {
            None
        } else {
            Some(Duration::from_millis(millis as u64))
        });
    }

    /// `socket.remoteAddress` — the empty string on a destroyed socket, which
    /// is how `std::http`'s `remote_address` flattens node's `undefined`.
    pub fn remote_address(&self) -> Str {
        if self.destroyed() {
            return str_new("");
        }
        str_new(&self.0.peer)
    }

    /// `socket.remoteAddress` AS the host value `std::http` declares it at
    /// (`remote_address_raw(self): JsonValue`), which is what its
    /// `remote_address` flattens: node answers `undefined` on a destroyed
    /// socket, and the vilan side tests `kind() == String` before coercing.
    /// `remote_address` above is the same fact already flattened, kept because
    /// it is what this runtime's own code reads.
    pub fn remote_address_raw(&self) -> JsonValue {
        if self.destroyed() {
            return JsonValue::Undefined;
        }
        JsonValue::Text(str_new(&self.0.peer))
    }

    /// `socket.destroy()`.
    pub fn destroy(&self) {
        self.0.destroy();
    }

    /// `socket.destroyed`.
    pub fn destroyed(&self) -> bool {
        self.0.destroyed.get() || self.0.stage.get() == Stage::Closed
    }
}

impl PartialEq for Socket {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Js for Socket {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a socket is a host object's own inspection, which the native backend does \
             not reproduce",
        )
    }
}

// ---------------------------------------------------------------- request ---

/// `NodeRequest` — one inbound request, head and body both already read.
///
/// The body is COLLECTED before the handler is called, which is what
/// `std::http`'s `Server::start` does on the JS backend too (it awaits
/// `node:stream/consumers`' `buffer` before constructing its `Request`). So
/// [`read_request_bytes`] hands back what is already here rather than driving a
/// stream.
#[derive(Clone)]
pub struct Request(Rc<RequestBody>);

struct RequestBody {
    method: String,
    target: String,
    fields: Vec<(String, String)>,
    body: Bytes,
}

impl Request {
    /// `request.url` — the request TARGET (path and query), which is what node
    /// puts there for a server request.
    pub fn url(&self) -> Str {
        str_new(&self.0.target)
    }

    /// `request.method`.
    pub fn method(&self) -> Str {
        str_new(&self.0.method)
    }

    /// One header's value, or `None` when the request did not carry it —
    /// `request.headers[name]` plus the `Object.hasOwn` presence test
    /// `std::http`'s `Request::header` performs on it. The name is lowered here
    /// too, so either casing asks the same question.
    pub fn header(&self, name: &str) -> Option<Str> {
        let wanted = name.to_ascii_lowercase();
        self.0
            .fields
            .iter()
            .find(|(known, _)| *known == wanted)
            .map(|(_, value)| str_new(value))
    }

    /// The whole body as bytes.
    pub fn bytes(&self) -> Bytes {
        self.0.body.clone()
    }

    /// `request.headers` — the whole field set as the opaque host object
    /// `std::http` declares it at, which `Request::header` reads named entries
    /// out of with `std::json`'s accessors (dynamic property access has no
    /// plain extern shape, which is why the binding answers a `JsonValue` and
    /// not a map).
    ///
    /// The names are already lowercased and repeats already joined, by the
    /// parser above, for the reason node lowercases and joins: `Request::header`
    /// is documented against that shape.
    pub fn headers(&self) -> JsonValue {
        JsonValue::object(
            self.0
                .fields
                .iter()
                .map(|(name, value)| (str_new(name), JsonValue::Text(str_new(value))))
                .collect(),
        )
    }
}

impl PartialEq for Request {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Js for Request {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a request is a host object's own inspection, which the native backend does \
             not reproduce",
        )
    }
}

/// `node:stream/consumers`' `buffer` — the body, which is already read.
pub async fn read_request_bytes(request: Request) -> Bytes {
    request.bytes()
}

// --------------------------------------------------------------- response ---

/// `NodeResponse` — the write half of one exchange.
#[derive(Clone)]
pub struct Response(Rc<ResponseBody>);

struct ResponseBody {
    connection: Rc<Connection>,
    status: Cell<u16>,
    headers: RefCell<Vec<(String, String)>>,
    /// Whether the status line and headers have gone out. A `setHeader` after
    /// that point is dropped, as node's is (it throws there; dropping is the
    /// conservative half of the divergence and is written down).
    head_sent: Cell<bool>,
    ended: Cell<bool>,
}

impl Response {
    /// `response.statusCode = code`.
    pub fn set_status_code(&self, code: i32) {
        self.0.status.set(code.clamp(100, 599) as u16);
    }

    /// `response.setHeader(name, value)` — last write wins per name, which is
    /// node's behaviour for the same call.
    pub fn set_header(&self, name: &str, value: &str) {
        if self.0.head_sent.get() {
            return;
        }
        let mut headers = self.0.headers.borrow_mut();
        match headers
            .iter_mut()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
        {
            Some((_, existing)) => *existing = value.to_string(),
            None => headers.push((name.to_string(), value.to_string())),
        }
    }

    /// Writes the status line and headers if they have not gone out yet.
    ///
    /// **The one place a byte comparison against node is impossible.** Node adds
    /// `Date` (which changes every second), and `Connection: keep-alive` plus
    /// `Keep-Alive: timeout=5` where this v1 says `Connection: close`. So the
    /// differential this server is held to compares the status line, the headers
    /// the PROGRAM set, and the body — not node's own additions. `Content-Length`
    /// is written for a buffered body because it is derivable and a length-
    /// delimited response is what lets a client read the body without a
    /// framing layer; a STREAM gets none, and its reader ends at the close.
    fn send_head(&self, content_length: Option<usize>) {
        if self.0.head_sent.replace(true) {
            return;
        }
        let status = self.0.status.get();
        let phrase = reason(status);
        let mut head = if phrase.is_empty() {
            format!("HTTP/1.1 {status}\r\n")
        } else {
            format!("HTTP/1.1 {status} {phrase}\r\n")
        };
        for (name, value) in self.0.headers.borrow().iter() {
            head.push_str(name);
            head.push_str(": ");
            head.push_str(value);
            head.push_str("\r\n");
        }
        if let Some(length) = content_length {
            head.push_str(&format!("Content-Length: {length}\r\n"));
        }
        head.push_str("Connection: close\r\n\r\n");
        self.0.connection.enqueue(head.as_bytes());
    }

    /// `response.write(chunk)` — a chunk without ending the response, which is
    /// SSE's shape. The head goes out on the first chunk, with no
    /// `Content-Length`, because a stream has no length to declare.
    pub fn write(&self, chunk: &str) {
        if self.0.ended.get() {
            return;
        }
        self.send_head(None);
        self.0.connection.enqueue(chunk.as_bytes());
    }

    /// `response.end(body)`.
    pub fn end(&self, body: &str) {
        self.end_bytes(&Bytes::from_vec(body.as_bytes().to_vec()));
    }

    /// `response.end(bytes)`.
    pub fn end_bytes(&self, body: &Bytes) {
        if self.0.ended.replace(true) {
            return;
        }
        // A response that already streamed declared no length, so its body
        // cannot gain one now: the reader is ending at the close either way.
        let length = if self.0.head_sent.get() {
            None
        } else {
            Some(body.len() as usize)
        };
        self.send_head(length);
        self.0.connection.enqueue(&body.as_slice());
        self.0.connection.stage.set(Stage::Closing);
    }

    /// `response.on("close", callback)` — the connection ended. For a
    /// never-ended streaming response that is the client going away, which is
    /// the event `std::http`'s `ResponseStream::on_close` exists for.
    pub fn on_event(&self, event: &str, callback: Rc<dyn Fn()>) {
        if event == "close" {
            self.0.connection.on_close.borrow_mut().push(callback);
        }
    }
}

impl PartialEq for Response {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Js for Response {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a response is a host object's own inspection, which the native backend \
             does not reproduce",
        )
    }
}

/// Answers `status` with an empty body and closes — the arm every protocol
/// refusal in [`parse_head`] and [`body_length`] takes. It writes the head by
/// hand rather than through [`Response`], because a refused request never
/// reaches the application and never gets a `Response` at all.
fn refuse(connection: &Rc<Connection>, status: u16, phrase: &str) {
    let head =
        format!("HTTP/1.1 {status} {phrase}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    connection.enqueue(head.as_bytes());
    connection.stage.set(Stage::Closing);
}

// ----------------------------------------------------------------- server ---

/// The request handler a server is built with: `createServer`'s callback.
///
/// It answers a FUTURE, because the callback `std::http` passes awaits (it reads
/// the body and then the application's `async` handler). Node ignores the
/// promise its callback returns, so this one is spawned and floating: a handler
/// that panics reports through the executor's unobserved-failure path and the
/// server goes on serving, which is node's behaviour for the same shape.
pub type Handler = Rc<dyn Fn(Request, Response) -> Boxed<()>>;

/// The upgrade handler: `(request, raw socket, head bytes)`.
///
/// SYNCHRONOUS, unlike [`Handler`], because that is how `std::http` declares it
/// (`|NodeRequest, NodeSocket, Bytes| void`). A40 made the vilan-level upgrade
/// handler allowed to suspend, and an rpc service's `authorize` hook uses it;
/// natively that is the adapted-instance shape (a declared-sync position holding
/// an async closure) and it is Order 40's, named here rather than guessed at.
pub type UpgradeHandler = Rc<dyn Fn(Request, Socket, Bytes)>;

/// `NodeServer` — what `createServer` answers.
#[derive(Clone)]
pub struct Server(Rc<ServerBody>);

struct ServerBody {
    handler: Handler,
    upgrade: RefCell<Option<UpgradeHandler>>,
    listener: RefCell<Option<TcpListener>>,
    connections: RefCell<Vec<Rc<Connection>>>,
    port: Cell<u16>,
    /// `server.close(callback)`: stop accepting, fire when the last connection
    /// has ended.
    closing: Cell<bool>,
    on_closed: RefCell<Option<Rc<dyn Fn()>>>,
}

/// `http.createServer(handler)`.
pub fn create_server(handler: Handler) -> Server {
    Server(Rc::new(ServerBody {
        handler,
        upgrade: RefCell::new(None),
        listener: RefCell::new(None),
        connections: RefCell::new(Vec::new()),
        port: Cell::new(0),
        closing: Cell::new(false),
        on_closed: RefCell::new(None),
    }))
}

/// `AddressInfo` — what a listening server reports. Only the port is read, and
/// only the port is here: `std::http`'s `NodeAddress` binds exactly `port`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Address {
    port: i32,
}

impl Address {
    pub fn port(&self) -> i32 {
        self.port
    }
}

impl Js for Address {
    fn js(&self) -> String {
        crate::panic_with(
            "printing an address is a host object's own inspection, which the native backend \
             does not reproduce",
        )
    }
}

impl Server {
    /// `server.listen(port, on_ready)`.
    ///
    /// Binds `127.0.0.1` and `::1`? No — one socket, on `0.0.0.0`, which is what
    /// node's `listen(port)` with no host does. Port `0` asks the OS for a free
    /// one, and [`Server::port`] reports what it gave, which is the whole
    /// mechanism `std::http`'s port-0 bind and every test harness here depends
    /// on.
    ///
    /// `on_ready` is called SYNCHRONOUSLY once the socket is bound and
    /// registered. Node defers it to the next tick; the difference is not
    /// observable through `std::http`, whose `listen` callback only reads the
    /// bound port and hands the server on, and making it a microtask would mean
    /// the announcement raced the first request.
    pub fn listen(&self, port: i32, on_ready: Rc<dyn Fn()>) {
        let requested = port.clamp(0, 65535) as u16;
        let listener = match TcpListener::bind(("0.0.0.0", requested)) {
            Ok(listener) => listener,
            Err(error) => crate::panic_with(&format!(
                "the server could not bind port {requested}: {error}"
            )),
        };
        if let Err(error) = listener.set_nonblocking(true) {
            crate::panic_with(&format!("the server's socket could not be polled: {error}"));
        }
        let bound = listener
            .local_addr()
            .map(|address| address.port())
            .unwrap_or(requested);
        self.0.port.set(bound);
        *self.0.listener.borrow_mut() = Some(listener);
        register_io(Rc::new(self.clone()));
        on_ready();
    }

    /// `server.address()`.
    pub fn address(&self) -> Address {
        Address {
            port: self.0.port.get() as i32,
        }
    }

    /// The port actually bound — what `address().port` reads.
    pub fn port(&self) -> i32 {
        self.0.port.get() as i32
    }

    /// `server.on("upgrade", handler)`.
    pub fn on_upgrade(&self, event: &str, handler: UpgradeHandler) {
        if event == "upgrade" {
            *self.0.upgrade.borrow_mut() = Some(handler);
        }
    }

    /// `server.close(on_closed)` — stop accepting and fire once the existing
    /// connections have ended.
    pub fn close(&self, on_closed: Rc<dyn Fn()>) {
        self.0.closing.set(true);
        *self.0.listener.borrow_mut() = None;
        *self.0.on_closed.borrow_mut() = Some(on_closed);
    }

    /// Accepts everything waiting. `true` if anything was accepted.
    fn accept_pending(&self) -> bool {
        let mut accepted = false;
        loop {
            let incoming = {
                let listener = self.0.listener.borrow();
                let Some(listener) = listener.as_ref() else {
                    return accepted;
                };
                listener.accept()
            };
            match incoming {
                Ok((stream, peer)) => {
                    if stream.set_nonblocking(true).is_err() {
                        continue;
                    }
                    // Nagle off: a response is written in one or two `write`s
                    // and then closed, so buffering it costs a round trip and
                    // buys nothing.
                    let _ = stream.set_nodelay(true);
                    self.0
                        .connections
                        .borrow_mut()
                        .push(Rc::new(Connection::new(stream, peer.ip().to_string())));
                    accepted = true;
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => return accepted,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                // A failed accept is not a failed server: the listener is still
                // bound and the next poll tries again.
                Err(_) => return accepted,
            }
        }
    }

    /// Reads, dispatches, writes and reaps one connection. `true` if anything
    /// progressed.
    fn service(&self, connection: &Rc<Connection>) -> bool {
        let mut progressed = false;
        let (read, hung_up) = connection.read_available();
        progressed |= read;
        match connection.stage.get() {
            Stage::Reading => {
                if self.dispatch(connection) {
                    progressed = true;
                } else if hung_up {
                    // The peer went away mid-head: nothing to answer.
                    connection.close();
                    progressed = true;
                }
            }
            Stage::Upgraded => {
                let taken: Vec<u8> = connection.inbox.borrow_mut().drain(..).collect();
                if !taken.is_empty() {
                    let frame = Bytes::from_vec(taken);
                    for listener in connection.on_bytes.borrow().iter() {
                        listener(frame.clone());
                    }
                    progressed = true;
                }
            }
            _ => {}
        }
        let (written, dead) = connection.write_available();
        progressed |= written;
        if dead {
            connection.close();
            return true;
        }
        if connection.stage.get() == Stage::Closing && connection.outbox.borrow().is_empty() {
            connection.close();
            progressed = true;
        }
        if hung_up
            && connection.outbox.borrow().is_empty()
            && connection.stage.get() != Stage::Dispatched
        {
            connection.close();
            progressed = true;
        }
        if self.fire_idle_timeout(connection) {
            progressed = true;
        }
        progressed
    }

    /// `socket.setTimeout`'s event, checked once per turn.
    fn fire_idle_timeout(&self, connection: &Rc<Connection>) -> bool {
        let Some(limit) = connection.idle_limit.get() else {
            return false;
        };
        if connection.timed_out.get() || connection.stage.get() == Stage::Closed {
            return false;
        }
        if connection.last_activity.get().elapsed() < limit {
            return false;
        }
        connection.timed_out.set(true);
        for listener in connection.on_timeout.borrow().iter() {
            listener();
        }
        true
    }

    /// If a whole request has arrived, hand it to the handler (or to the upgrade
    /// handler). `true` if one was dispatched or refused.
    fn dispatch(&self, connection: &Rc<Connection>) -> bool {
        let buffered: Vec<u8> = connection.inbox.borrow().clone();
        let head = match parse_head(&buffered) {
            Ok(head) => head,
            Err(HeadError::Incomplete) => return false,
            Err(HeadError::Refused(status, phrase)) => {
                refuse(connection, status, phrase);
                return true;
            }
        };
        let length = match body_length(&head) {
            Ok(length) => length,
            Err(HeadError::Refused(status, phrase)) => {
                refuse(connection, status, phrase);
                return true;
            }
            // `body_length` never asks for more bytes.
            Err(HeadError::Incomplete) => return false,
        };
        let upgrade = asks_to_upgrade(&head);
        if !upgrade && buffered.len() < head.length + length {
            return false;
        }
        let body = if upgrade {
            // The upgrade's "head" is whatever arrived after the request head —
            // node's third argument, and for a WebSocket handshake it is almost
            // always empty.
            buffered[head.length..].to_vec()
        } else {
            buffered[head.length..head.length + length].to_vec()
        };
        connection.inbox.borrow_mut().clear();
        let request = Request(Rc::new(RequestBody {
            method: head.method,
            target: head.target,
            fields: head.fields,
            body: Bytes::from_vec(body.clone()),
        }));
        if upgrade {
            let handler = self.0.upgrade.borrow().clone();
            match handler {
                Some(handler) => {
                    connection.stage.set(Stage::Upgraded);
                    let socket = Socket(Rc::clone(connection));
                    handler(request, socket, Bytes::from_vec(body));
                }
                // Node destroys an unclaimed upgrade socket, and so does
                // `std::http`'s own documentation of this path.
                None => connection.close(),
            }
            return true;
        }
        connection.stage.set(Stage::Dispatched);
        let response = Response(Rc::new(ResponseBody {
            connection: Rc::clone(connection),
            status: Cell::new(200),
            headers: RefCell::new(Vec::new()),
            head_sent: Cell::new(false),
            ended: Cell::new(false),
        }));
        spawn(
            (self.0.handler)(request, response),
            "an http request handler",
        );
        true
    }
}

impl IoSource for Server {
    fn poll(&self) -> bool {
        let mut progressed = self.accept_pending();
        let connections: Vec<Rc<Connection>> = self.0.connections.borrow().clone();
        for connection in &connections {
            if self.service(connection) {
                progressed = true;
            }
        }
        self.0
            .connections
            .borrow_mut()
            .retain(|connection| connection.stage.get() != Stage::Closed);
        if self.0.closing.get() && self.0.connections.borrow().is_empty() {
            let callback = self.0.on_closed.borrow_mut().take();
            if let Some(callback) = callback {
                callback();
                progressed = true;
            }
        }
        progressed
    }

    fn is_live(&self) -> bool {
        // A closed server with no connections left has nothing more to do, and
        // answering `false` is what lets the program EXIT: the loop drops the
        // source and goes back to exiting when the deadline list empties.
        self.0.listener.borrow().is_some() || !self.0.connections.borrow().is_empty()
    }
}

impl PartialEq for Server {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Js for Server {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a server is a host object's own inspection, which the native backend does \
             not reproduce",
        )
    }
}

/// The five host types this module IS are opaque objects on the JS backend — a
/// `NodeRequest` is an `http.IncomingMessage` — and `JSON.stringify` of an
/// object whose own enumerable properties are all functions or absent is `{}`.
/// They exist as [`crate::Json`] only so that a vilan struct holding one can
/// still carry the `impl Json` the emitter writes beside every `impl Js` (F20).
/// Anything that needs a real rendering of a request is reading the wrong type.
macro_rules! opaque_json {
    ($($type:ty),*) => {
        $(impl crate::Json for $type {
            fn json(&self) -> String {
                "{}".to_string()
            }
        })*
    };
}

opaque_json!(Server, Address, Request, Response, Socket);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::block_on;
    use std::io::{BufRead, BufReader};

    // -- the parser, per rule --

    fn head_of(text: &str) -> Result<Head, HeadError> {
        parse_head(text.as_bytes())
    }

    #[test]
    fn a_request_line_and_its_fields_parse_with_the_names_lowercased() {
        let head = head_of("GET /x?y=1 HTTP/1.1\r\nHost: a\r\nX-Mixed-Case: v\r\n\r\n")
            .expect("a whole head parses");
        assert_eq!(head.method, "GET");
        assert_eq!(head.target, "/x?y=1");
        assert_eq!(
            head.fields,
            vec![
                ("host".to_string(), "a".to_string()),
                ("x-mixed-case".to_string(), "v".to_string()),
            ]
        );
        assert_eq!(head.length, 49);
    }

    #[test]
    fn a_head_that_has_not_finished_arriving_asks_for_more() {
        assert_eq!(
            head_of("GET / HTTP/1.1\r\nHost: a\r\n"),
            Err(HeadError::Incomplete)
        );
    }

    #[test]
    fn a_bare_line_feed_terminates_a_line_too() {
        let head = head_of("GET / HTTP/1.1\nHost: a\n\n").expect("bare LF is accepted");
        assert_eq!(head.target, "/");
        assert_eq!(head.fields, vec![("host".to_string(), "a".to_string())]);
    }

    #[test]
    fn a_repeated_field_is_joined_the_way_node_presents_it() {
        let head = head_of("GET / HTTP/1.1\r\nX-Dup: one\r\nX-Dup: two\r\n\r\n").expect("parses");
        assert_eq!(
            head.fields,
            vec![("x-dup".to_string(), "one, two".to_string())]
        );
        // `set-cookie` is the one node keeps as a list, and `std::http` documents
        // it as arriving comma-joined WITHOUT the space.
        let cookies = head_of("GET / HTTP/1.1\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\n\r\n")
            .expect("parses");
        assert_eq!(
            cookies.fields,
            vec![("set-cookie".to_string(), "a=1,b=2".to_string())]
        );
    }

    #[test]
    fn a_malformed_head_is_refused_with_the_status_it_earns() {
        assert_eq!(
            head_of("nonsense\r\n\r\n"),
            Err(HeadError::Refused(400, "Bad Request"))
        );
        assert_eq!(
            head_of("GET / HTTP/1.1\r\nno-colon\r\n\r\n"),
            Err(HeadError::Refused(400, "Bad Request"))
        );
        // RFC 9112 §5.1: whitespace before the colon is a must-reject.
        assert_eq!(
            head_of("GET / HTTP/1.1\r\nHost : a\r\n\r\n"),
            Err(HeadError::Refused(400, "Bad Request"))
        );
        let long = format!("GET / HTTP/1.1\r\nX: {}\r\n", "a".repeat(MAX_HEAD));
        assert_eq!(
            parse_head(long.as_bytes()),
            Err(HeadError::Refused(431, "Request Header Fields Too Large"))
        );
    }

    #[test]
    fn a_body_length_is_read_and_a_chunked_request_is_refused() {
        let with = head_of("POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\n").expect("parses");
        assert_eq!(body_length(&with), Ok(5));
        let without = head_of("GET / HTTP/1.1\r\n\r\n").expect("parses");
        assert_eq!(body_length(&without), Ok(0));
        let chunked =
            head_of("POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n").expect("parses");
        assert_eq!(
            body_length(&chunked),
            Err(HeadError::Refused(411, "Length Required"))
        );
        let nonsense = head_of("POST / HTTP/1.1\r\nContent-Length: ten\r\n\r\n").expect("parses");
        assert_eq!(
            body_length(&nonsense),
            Err(HeadError::Refused(400, "Bad Request"))
        );
        let huge = head_of(&format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        ))
        .expect("parses");
        assert_eq!(
            body_length(&huge),
            Err(HeadError::Refused(413, "Content Too Large"))
        );
    }

    #[test]
    fn an_upgrade_is_recognised_only_with_both_fields() {
        let both = head_of("GET / HTTP/1.1\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n")
            .expect("parses");
        assert!(asks_to_upgrade(&both));
        // A `Connection` field listing several tokens still counts.
        let listed =
            head_of("GET / HTTP/1.1\r\nConnection: keep-alive, Upgrade\r\nUpgrade: h2c\r\n\r\n")
                .expect("parses");
        assert!(asks_to_upgrade(&listed));
        let one = head_of("GET / HTTP/1.1\r\nUpgrade: websocket\r\n\r\n").expect("parses");
        assert!(!asks_to_upgrade(&one));
        let plain = head_of("GET / HTTP/1.1\r\nConnection: close\r\n\r\n").expect("parses");
        assert!(!asks_to_upgrade(&plain));
    }

    // -- the server, over a real socket --

    /// Speaks one request to `port` from a thread and answers the whole
    /// response text. A THREAD, because the executor owns this one and a
    /// blocking read here would deadlock the server it is talking to.
    fn fetch(port: u16, request: &str) -> std::thread::JoinHandle<String> {
        let request = request.to_string();
        std::thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
            stream.write_all(request.as_bytes()).expect("send");
            let mut response = String::new();
            stream.read_to_string(&mut response).expect("read");
            response
        })
    }

    /// The status line and the body of a response, with node's unreproducible
    /// additions left out of the comparison.
    fn split(response: &str) -> (String, Vec<String>, String) {
        let mut lines = BufReader::new(response.as_bytes())
            .lines()
            .map_while(Result::ok);
        let status = lines.next().unwrap_or_default();
        let mut headers = Vec::new();
        for line in lines.by_ref() {
            if line.is_empty() {
                break;
            }
            headers.push(line);
        }
        let body = response
            .split_once("\r\n\r\n")
            .map(|(_, rest)| rest.to_string())
            .unwrap_or_default();
        (status, headers, body)
    }

    #[test]
    fn a_get_is_answered_with_the_status_headers_and_body_the_handler_set() {
        let served = Rc::new(RefCell::new(Vec::<String>::new()));
        let seen = Rc::clone(&served);
        let answered = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                let seen = Rc::clone(&seen);
                crate::executor::pin_future(async move {
                    seen.borrow_mut()
                        .push(format!("{} {}", request.method(), request.url()));
                    response.set_status_code(201);
                    response.set_header("Content-Type", "text/plain");
                    response.end("hello\n");
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            let client = fetch(port, "GET /x HTTP/1.1\r\nHost: a\r\n\r\n");
            // The executor keeps turning while the client's thread talks: a
            // `sleep` is the only wait here, and the response has to have gone
            // out by the time it returns, which is what the join asserts.
            crate::executor::sleep(200, None).await;
            server.close(Rc::new(|| {}));
            client.join().expect("the client thread")
        });
        let (status, headers, body) = split(&answered);
        assert_eq!(status, "HTTP/1.1 201 Created");
        assert!(
            headers
                .iter()
                .any(|line| line == "Content-Type: text/plain"),
            "the handler's header must go out: {headers:?}"
        );
        assert!(
            headers.iter().any(|line| line == "Content-Length: 6"),
            "a buffered body declares its length: {headers:?}"
        );
        assert_eq!(body, "hello\n");
        assert_eq!(served.borrow().clone(), vec!["GET /x".to_string()]);
    }

    #[test]
    fn a_post_body_reaches_the_handler_whole() {
        let answered = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                crate::executor::pin_future(async move {
                    let body = request.bytes();
                    let text = String::from_utf8_lossy(&body.as_slice()).into_owned();
                    let host = request
                        .header("HOST")
                        .map(|value| value.to_string())
                        .unwrap_or_default();
                    response.end(&format!("{text}|{host}"));
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            // The body arrives in a SECOND write, after the head, so the
            // connection has to keep reading rather than dispatching early.
            let client = std::thread::spawn(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                stream
                    .write_all(b"POST /p HTTP/1.1\r\nHost: h\r\nContent-Length: 11\r\n\r\n")
                    .expect("send the head");
                std::thread::sleep(Duration::from_millis(20));
                stream.write_all(b"hello world").expect("send the body");
                let mut response = String::new();
                stream.read_to_string(&mut response).expect("read");
                response
            });
            crate::executor::sleep(300, None).await;
            server.close(Rc::new(|| {}));
            client.join().expect("the client thread")
        });
        let (status, _, body) = split(&answered);
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert_eq!(body, "hello world|h");
    }

    #[test]
    fn a_chunked_request_is_refused_over_the_wire_rather_than_misread() {
        let answered = block_on(async move {
            let handler: Handler = Rc::new(move |_request: Request, response: Response| {
                crate::executor::pin_future(async move {
                    response.end("the handler must not be reached");
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            let client = fetch(
                port,
                "POST / HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
            );
            crate::executor::sleep(200, None).await;
            server.close(Rc::new(|| {}));
            client.join().expect("the client thread")
        });
        let (status, _, body) = split(&answered);
        assert_eq!(status, "HTTP/1.1 411 Length Required");
        assert_eq!(body, "");
    }

    #[test]
    fn a_streaming_response_sends_its_chunks_and_declares_no_length() {
        let answered = block_on(async move {
            let handler: Handler = Rc::new(move |_request: Request, response: Response| {
                crate::executor::pin_future(async move {
                    response.set_header("Content-Type", "text/event-stream");
                    response.write("data: one\n\n");
                    crate::executor::sleep(10, None).await;
                    response.write("data: two\n\n");
                    response.end("");
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            let client = fetch(port, "GET /events HTTP/1.1\r\nHost: a\r\n\r\n");
            crate::executor::sleep(300, None).await;
            server.close(Rc::new(|| {}));
            client.join().expect("the client thread")
        });
        let (status, headers, body) = split(&answered);
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert!(
            !headers
                .iter()
                .any(|line| line.starts_with("Content-Length")),
            "a stream declares no length: {headers:?}"
        );
        assert_eq!(body, "data: one\n\ndata: two\n\n");
    }

    #[test]
    fn the_loop_keeps_running_timers_while_a_server_is_bound() {
        // The claim the I/O turn is about: a bound server does not starve the
        // deadline list, and a due timer does not starve the socket.
        let ticks = Rc::new(Cell::new(0u32));
        let counted = Rc::clone(&ticks);
        let answered = block_on(async move {
            let handler: Handler = Rc::new(move |_request: Request, response: Response| {
                crate::executor::pin_future(async move {
                    response.end("pong");
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            let client = fetch(port, "GET / HTTP/1.1\r\nHost: a\r\n\r\n");
            for _ in 0..5 {
                crate::executor::sleep(10, None).await;
                counted.set(counted.get() + 1);
            }
            crate::executor::sleep(150, None).await;
            server.close(Rc::new(|| {}));
            client.join().expect("the client thread")
        });
        assert_eq!(ticks.get(), 5, "the timers must still fire");
        assert_eq!(split(&answered).2, "pong");
    }

    #[test]
    fn a_closed_server_lets_the_program_exit() {
        // `is_live` answering `false` is what ends the loop; a server that
        // stayed registered would hang `block_on` forever, so this test
        // TERMINATING is the assertion.
        let stopped = Rc::new(Cell::new(false));
        let flag = Rc::clone(&stopped);
        block_on(async move {
            let handler: Handler = Rc::new(move |_request: Request, response: Response| {
                crate::executor::pin_future(async move { response.end("") })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let closed = Rc::clone(&flag);
            server.close(Rc::new(move || closed.set(true)));
            crate::executor::sleep(20, None).await;
        });
        assert!(stopped.get(), "`close`'s callback must fire");
    }

    #[test]
    fn an_upgrade_reaches_its_handler_with_the_socket_and_the_head() {
        let taken = Rc::new(RefCell::new(String::new()));
        let recorded = Rc::clone(&taken);
        block_on(async move {
            let handler: Handler = Rc::new(move |_request: Request, response: Response| {
                crate::executor::pin_future(async move { response.end("not the upgrade path") })
            });
            let server = create_server(handler);
            let seen = Rc::clone(&recorded);
            server.on_upgrade(
                "upgrade",
                Rc::new(move |request: Request, socket: Socket, head: Bytes| {
                    *seen.borrow_mut() = format!(
                        "{} head={}",
                        request.url(),
                        String::from_utf8_lossy(&head.as_slice())
                    );
                    socket.write_text("HTTP/1.1 101 Switching Protocols\r\n\r\n");
                    socket.destroy();
                }),
            );
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            let client = fetch(
                port,
                "GET /ws HTTP/1.1\r\nHost: a\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n",
            );
            crate::executor::sleep(200, None).await;
            server.close(Rc::new(|| {}));
            let response = client.join().expect("the client thread");
            assert!(
                response.starts_with("HTTP/1.1 101 Switching Protocols"),
                "the upgrade handler owns the socket: {response:?}"
            );
        });
        assert_eq!(taken.borrow().clone(), "/ws head=");
    }
}
