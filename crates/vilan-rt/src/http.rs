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
//! `vilan/std/src/http.vl` binds `node:http` — `createServer`, the
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
//! accepts; a connection reads until it has a whole request HEAD; the request
//! handler is SPAWNED, exactly as node treats the promise its callback returns
//! (floating — an unhandled rejection reports and the server keeps serving); the
//! response's bytes go into an outbox the same poll flushes. Nothing blocks the
//! loop, so a program can serve requests and run timers at once.
//!
//! # The body is read when the handler asks (M164)
//!
//! A handler runs on the head. Its body stays in the kernel — the connection does
//! not read it, so a client that sends more than the socket buffers holds stalls
//! — until `std::http`'s `Request::bytes` asks ([`read_body`]), under the cap
//! the server resolved for the request: a declared `Content-Length` over it is
//! refused before a byte is read, and a chunked body (decoded here, RFC 9112
//! §7.1) is counted as it arrives and refused at the chunk size that crosses it.
//! A refusal answers `413` and closes; the read ends its task as a cancellation.
//! A body nobody read is discarded when it is within the cap (so the client
//! reads its response rather than a reset, and a kept-alive connection reaches
//! the next request), and the connection is closed at once when it is not.
//!
//! # Persistent connections (F152, Order 58's S3)
//!
//! A connection serves requests one after another, as node's does (RFC 9112
//! §9.3): an HTTP/1.1 request keeps it unless either side says `Connection:
//! close`, an HTTP/1.0 request only with `Connection: keep-alive` and a response
//! whose length is declared. Each answer says which in node's own fields —
//! `Connection: keep-alive` and `Keep-Alive: timeout=N`, or `Connection: close` —
//! and a response with no declared length is `Transfer-Encoding: chunked` where
//! node's is. Requests a client PIPELINES are answered strictly in order: the
//! next one is read off the connection only once the previous answer has ended,
//! its body has been read or discarded, and its bytes have left for the kernel,
//! so what one client queues is bounded by one response. A request that upgrades
//! leaves the loop: the socket is the upgrade handler's from then on.
//!
//! Three deadlines bound a connection, each node's by name and default
//! ([`ServerPolicy`]): the HEAD must arrive within `headersTimeout` (60 s) of the
//! request's first byte — of the accept, for a fresh connection, so one that sends
//! nothing is bounded too; the whole REQUEST, its body included, within
//! `requestTimeout` (300 s); and a connection idle between requests closes after
//! `keepAliveTimeout` (5 s) plus node's one-second grace. A request that misses
//! its deadline is answered node's `408 Request Timeout` (when nothing has been
//! answered yet), its body read in flight ends as a cancellation, and the
//! connection closes. Node sweeps for expired requests on an interval; this
//! server checks every turn, so it is the more punctual of the two.
//!
//! # Not here, by name
//!
//! HTTP/2, TLS (a native server is deployed behind a proxy that terminates it;
//! `native-objective.md` S1), node's `Date` field, and the WebSocket handshake
//! itself — the upgrade HANDOVER is here, so a `std::ws` written in vilan has a
//! socket to write its handshake to, but nothing here computes an RFC 6455
//! accept key.

use std::cell::{Cell, RefCell};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::bytes::Bytes;
use crate::executor::{
    Boxed, Failure, IoSource, Parked, caught, raise_cancelled, raise_failure, register_io, spawn,
    termination_requested,
};
use crate::json::JsonValue;
use crate::{Js, Str, str_new};

/// One read of a connection. 8 KiB is node's own default highWaterMark for a
/// socket, which is the only reason to prefer it to any other number.
const READ_CHUNK: usize = 8 * 1024;

/// The largest request head this accepts before answering `431`. A head is
/// buffered whole before it can be parsed, so an unbounded one is an unbounded
/// allocation driven by a stranger.
const MAX_HEAD: usize = 64 * 1024;

/// How long a connection closing with body bytes possibly still on the wire — a
/// refused body, or one over the cap that the handler never read — waits, write
/// half shut and reading nothing, before it closes (M164). Closing a socket with
/// unread bytes in it sends a RESET rather than a FIN, and a reset can reach
/// the client before it has read the answer it was sent — the `413` lost on the
/// way. The wait is Go's `rstAvoidanceDelay` (500 ms): the client has the answer
/// and the end of it, and the server holds nothing but the socket for it.
const LINGER: Duration = Duration::from_millis(500);

/// `server.headersTimeout`'s default: a request head not whole 60 s after the
/// request began is answered `408` and its connection closed.
const DEFAULT_HEADERS_TIMEOUT: Duration = Duration::from_secs(60);

/// `server.requestTimeout`'s default: a request, body included, not whole 300 s
/// after it began is answered `408` (or, already answered, cut) and closed.
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

/// `server.keepAliveTimeout`'s default: how long a connection may sit idle
/// between requests — what `Keep-Alive: timeout=5` announces.
const DEFAULT_KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(5);

/// What node waits past `keepAliveTimeout` before it closes an idle connection
/// (its internal `keepAliveTimeoutBuffer`, measured on node 24: a 600 ms timeout
/// closes at 1.6 s). The announced timeout is the client's, the grace is the
/// server's: a client that sends just inside it does not race the close.
const KEEP_ALIVE_GRACE: Duration = Duration::from_secs(1);

/// The answer to a request that missed its deadline — node's own bytes
/// (`requestTimeoutResponse` in `_http_server.js`), written before the close.
const REQUEST_TIMEOUT_RESPONSE: &[u8] =
    b"HTTP/1.1 408 Request Timeout\r\nConnection: close\r\n\r\n";

/// How much of a body one turn reads before decoding it (M164): what a body over
/// its cap can have in memory when it is refused, beyond the cap itself.
const BODY_SLICE: usize = 64 * 1024;

/// The longest chunk-size line (or trailer line) a chunked body may carry before
/// it is refused as malformed: a size is a few hex digits, and an unbounded line
/// is an unbounded allocation driven by a stranger, as an unbounded head is.
const MAX_CHUNK_LINE: usize = 4 * 1024;

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
    /// Whether the request line said `HTTP/1.1`. Anything else is answered on
    /// HTTP/1.0's terms, as node answers it (a `HTTP/2.0` request line is a 1.0
    /// exchange to node's parser, not an h2 one).
    http11: bool,
    /// How many bytes of the head were consumed, so the body starts there.
    length: usize,
}

impl Head {
    /// One field's value, by its lowercased name.
    fn field(&self, wanted: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(name, _)| name == wanted)
            .map(|(_, value)| value.as_str())
    }

    /// Whether the CLIENT asks for the connection to persist past this request
    /// (RFC 9112 §9.3): HTTP/1.1 unless it says `close`, HTTP/1.0 only when it
    /// says `keep-alive`. The response has the last word ([`Response`]).
    fn wants_to_persist(&self) -> bool {
        let connection = self.field("connection").unwrap_or("");
        if has_token(connection, "close") {
            return false;
        }
        self.http11 || has_token(connection, "keep-alive")
    }
}

/// Whether a comma-separated field value (`Connection`'s) lists `token`,
/// ignoring case.
fn has_token(value: &str, token: &str) -> bool {
    value
        .split(',')
        .any(|listed| listed.trim().eq_ignore_ascii_case(token))
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
    // The version decides the connection's terms (RFC 9112 §9.3) and nothing
    // else: anything but `HTTP/1.1` is answered as 1.0 is.
    let http11 = parts.next() == Some("HTTP/1.1");
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
        http11,
        length,
    })
}

/// How a request's body is delimited on the wire (RFC 9112 §6.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Framing {
    /// Neither field: no body.
    Empty,
    /// `Content-Length: n`.
    Length(usize),
    /// `Transfer-Encoding: chunked`, decoded by [`ChunkDecoder`].
    Chunked,
}

/// The body's framing, or a refusal.
///
/// No size is refused here: the cap is the APPLICATION's (`std::http`'s
/// `max_body_bytes`, resolved per request), so it is applied when the body is
/// read ([`read_body`]), against the declared length first. A transfer coding
/// other than `chunked` is `501` (RFC 9112 §6.1: a coding the server does not
/// understand), and a request carrying both fields is `400` — the two framings
/// disagreeing about where the message ends is how requests are smuggled, and
/// node's parser refuses it too.
fn body_framing(head: &Head) -> Result<Framing, HeadError> {
    let field = |wanted: &str| {
        head.fields
            .iter()
            .find(|(name, _)| name == wanted)
            .map(|(_, value)| value.trim().to_ascii_lowercase())
    };
    let length = field("content-length");
    if let Some(coding) = field("transfer-encoding") {
        if length.is_some() {
            return Err(HeadError::Refused(400, "Bad Request"));
        }
        let codings: Vec<&str> = coding.split(',').map(str::trim).collect();
        if codings != ["chunked"] {
            return Err(HeadError::Refused(501, "Not Implemented"));
        }
        return Ok(Framing::Chunked);
    }
    let Some(value) = length else {
        return Ok(Framing::Empty);
    };
    match value.parse::<usize>() {
        Ok(0) => Ok(Framing::Empty),
        Ok(length) => Ok(Framing::Length(length)),
        Err(_) => Err(HeadError::Refused(400, "Bad Request")),
    }
}

/// Where a chunked body's decoder is (RFC 9112 §7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChunkStage {
    /// Reading a chunk-size line (`1a;ext=1`).
    Size,
    /// Inside a chunk's data, with this many bytes of it left.
    Data(usize),
    /// After a chunk's data: its line terminator.
    DataEnd,
    /// After the last (zero-size) chunk: trailer lines, up to an empty one.
    Trailers,
    /// The body has ended.
    Done,
}

/// What one [`ChunkDecoder::feed`] came to.
#[derive(Debug, PartialEq, Eq)]
enum Decoded {
    /// Everything available was taken; the body has not ended.
    More,
    /// The body ended.
    Done,
    /// A chunk size would take the body past the cap. Refused at the SIZE
    /// line, before any of that chunk's data is read.
    OverLimit,
    /// Not a chunked body.
    Malformed,
}

/// A chunked body, decoded incrementally out of the connection's inbox.
#[derive(Debug)]
struct ChunkDecoder {
    stage: ChunkStage,
    /// Trailer bytes taken so far, bounded by [`MAX_HEAD`].
    trailer_bytes: usize,
}

impl ChunkDecoder {
    fn new() -> ChunkDecoder {
        ChunkDecoder {
            stage: ChunkStage::Size,
            trailer_bytes: 0,
        }
    }

    /// Takes what it can out of `input` (draining what it consumed), appending
    /// the decoded data to `output`, and refuses a chunk whose size would take
    /// `output` past `limit` bytes.
    fn feed(&mut self, input: &mut Vec<u8>, output: &mut Vec<u8>, limit: usize) -> Decoded {
        let mut consumed = 0;
        let outcome = loop {
            let available = &input[consumed..];
            match self.stage {
                ChunkStage::Done => break Decoded::Done,
                ChunkStage::Size | ChunkStage::DataEnd | ChunkStage::Trailers => {
                    let Some(end) = available.iter().position(|byte| *byte == b'\n') else {
                        if available.len() > MAX_CHUNK_LINE {
                            break Decoded::Malformed;
                        }
                        break Decoded::More;
                    };
                    let line = &available[..end];
                    let line = line.strip_suffix(b"\r").unwrap_or(line);
                    consumed += end + 1;
                    match self.stage {
                        ChunkStage::Size => {
                            let Some(size) = chunk_size(line) else {
                                break Decoded::Malformed;
                            };
                            if size == 0 {
                                self.stage = ChunkStage::Trailers;
                                continue;
                            }
                            match output.len().checked_add(size) {
                                Some(total) if total <= limit => {}
                                _ => break Decoded::OverLimit,
                            }
                            self.stage = ChunkStage::Data(size);
                        }
                        ChunkStage::DataEnd => {
                            if !line.is_empty() {
                                break Decoded::Malformed;
                            }
                            self.stage = ChunkStage::Size;
                        }
                        _ => {
                            self.trailer_bytes += end + 1;
                            if self.trailer_bytes > MAX_HEAD {
                                break Decoded::Malformed;
                            }
                            if line.is_empty() {
                                self.stage = ChunkStage::Done;
                            }
                        }
                    }
                }
                ChunkStage::Data(remaining) => {
                    if available.is_empty() {
                        break Decoded::More;
                    }
                    let taken = remaining.min(available.len());
                    output.extend_from_slice(&available[..taken]);
                    consumed += taken;
                    self.stage = if taken == remaining {
                        ChunkStage::DataEnd
                    } else {
                        ChunkStage::Data(remaining - taken)
                    };
                }
            }
        };
        input.drain(..consumed);
        outcome
    }
}

/// A chunk-size line's size: hex digits, then optional `;` extensions (which
/// carry nothing this server reads). `None` for anything else, an empty size or
/// one too large for a `usize` among it.
fn chunk_size(line: &[u8]) -> Option<usize> {
    let text = std::str::from_utf8(line).ok()?;
    let digits = text.split(';').next()?.trim();
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    usize::from_str_radix(digits, 16).ok()
}

/// Whether the head asks for a protocol upgrade (`node:http`'s `upgrade` event,
/// which fires on `Connection: upgrade` with an `Upgrade` field).
fn asks_to_upgrade(head: &Head) -> bool {
    head.field("upgrade").is_some()
        && head
            .field("connection")
            .is_some_and(|value| has_token(value, "upgrade"))
}

/// The reason phrase for a status code: node's `http.STATUS_CODES`, whole, and
/// node's `unknown` for a code it does not list (F151, Order 58's S4).
///
/// NODE's table rather than RFC 9110's, because the two backends answer one wire:
/// node still says `413 Payload Too Large`, `422 Unprocessable Entity` and
/// `418 I'm a Teapot` where RFC 9110 renamed them, and an app's `413` read
/// `Payload Too Large` on node and `Content Too Large` here until this table
/// followed node's. Copied from node 24's `STATUS_CODES`; a code node adds later
/// is `unknown` here until this table learns it, which the differential's
/// status-line pin would show.
fn reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        102 => "Processing",
        103 => "Early Hints",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        203 => "Non-Authoritative Information",
        204 => "No Content",
        205 => "Reset Content",
        206 => "Partial Content",
        207 => "Multi-Status",
        208 => "Already Reported",
        226 => "IM Used",
        300 => "Multiple Choices",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        305 => "Use Proxy",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        407 => "Proxy Authentication Required",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        411 => "Length Required",
        412 => "Precondition Failed",
        413 => "Payload Too Large",
        414 => "URI Too Long",
        415 => "Unsupported Media Type",
        416 => "Range Not Satisfiable",
        417 => "Expectation Failed",
        418 => "I'm a Teapot",
        421 => "Misdirected Request",
        422 => "Unprocessable Entity",
        423 => "Locked",
        424 => "Failed Dependency",
        425 => "Too Early",
        426 => "Upgrade Required",
        428 => "Precondition Required",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        451 => "Unavailable For Legal Reasons",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        505 => "HTTP Version Not Supported",
        506 => "Variant Also Negotiates",
        507 => "Insufficient Storage",
        508 => "Loop Detected",
        509 => "Bandwidth Limit Exceeded",
        510 => "Not Extended",
        511 => "Network Authentication Required",
        _ => "unknown",
    }
}

/// The server's connection policy: node's three timeouts, and whether the
/// server is stopping. Shared by the server and every connection it accepted
/// (an `Rc` of its own, so a connection never holds its server and the two
/// never keep each other alive).
///
/// `None` is a deadline switched off — node's `0`.
struct ServerPolicy {
    headers_timeout: Cell<Option<Duration>>,
    request_timeout: Cell<Option<Duration>>,
    keep_alive_timeout: Cell<Option<Duration>>,
    /// `close()` or a termination request has begun: no exchange persists past
    /// its answer, and an idle connection is closed.
    stopping: Cell<bool>,
}

impl ServerPolicy {
    fn node_defaults() -> ServerPolicy {
        ServerPolicy {
            headers_timeout: Cell::new(Some(DEFAULT_HEADERS_TIMEOUT)),
            request_timeout: Cell::new(Some(DEFAULT_REQUEST_TIMEOUT)),
            keep_alive_timeout: Cell::new(Some(DEFAULT_KEEP_ALIVE_TIMEOUT)),
            stopping: Cell::new(false),
        }
    }

    /// A node timeout property's value as a deadline: `0` (or less) is none.
    fn deadline_of(millis: i32) -> Option<Duration> {
        (millis > 0).then(|| Duration::from_millis(millis as u64))
    }

    /// The deadline a request still reading its HEAD has: the nearer of the
    /// head's and the whole request's.
    fn head_deadline(&self) -> Option<Duration> {
        match (self.headers_timeout.get(), self.request_timeout.get()) {
            (Some(head), Some(request)) => Some(head.min(request)),
            (head, request) => head.or(request),
        }
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
    /// The response ended and the connection persists: what the request's body
    /// still owes is being discarded, and then the connection reads the next
    /// request ([`Connection::next_exchange`]).
    Settling,
    /// The response ended; flush what is left and close.
    Closing,
    /// Handed over to an upgrade handler: reads go to its `on_bytes`.
    Upgraded,
    /// Gone.
    Closed,
}

/// Where a request's body stands (M164).
enum BodyState {
    /// Nobody has asked for it: it stays on the wire (in the kernel, or the part
    /// that arrived with the head in the inbox).
    Unread,
    /// A handler asked: it is being taken off the wire, up to `limit` bytes.
    Reading { limit: usize },
    /// A handler is reading it as a STREAM (M176): it is taken off the wire only
    /// while a reader waits with nothing to take, up to `limit` bytes in all.
    /// `ended` once the framing says the body is over — what is still in
    /// [`Body::decoded`] is the readers' to take.
    Streaming { limit: usize, ended: bool },
    /// A stream the handler's answer left unfinished (M176): nothing reads it
    /// any more, and the close treats its rest as an unread body's — drained
    /// within the cap, never read past it.
    Abandoned,
    /// Read whole.
    Read(Bytes),
    /// The response ended with it unread and within the cap the server resolved:
    /// this many bytes are discarded before the connection closes, so the client
    /// reads its response rather than a reset.
    Draining { remaining: usize },
    /// Over the cap or malformed: answered, and the connection is closing.
    Refused,
    /// The client went away before it arrived, or the request was answered
    /// before it was asked for.
    Gone,
}

/// One request's body: its framing, where it stands, what has been decoded and
/// who waits for it.
struct Body {
    framing: Framing,
    state: BodyState,
    /// Decoded and not yet handed out: a whole read keeps all of it, a stream
    /// hands it out from [`Body::taken`] on.
    decoded: Vec<u8>,
    /// How much of [`Body::decoded`] a stream's readers have already taken.
    taken: usize,
    /// How many body bytes the framing has produced so far — what the cap
    /// counts, however many of them a stream has already handed out.
    received: usize,
    chunks: ChunkDecoder,
    waiters: Vec<Parked>,
    /// The cap [`Request::body_left_unread`] was asked with — what the drain
    /// spends at most.
    drain_limit: Option<usize>,
    /// How a handler claimed the body (`"whole"` or `"stream"`, M176): `std::http`
    /// reads it one way or the other, and this is what remembers which.
    claim: String,
}

impl Body {
    fn new(framing: Framing) -> Body {
        Body {
            framing,
            state: BodyState::Unread,
            decoded: Vec::new(),
            taken: 0,
            received: 0,
            chunks: ChunkDecoder::new(),
            waiters: Vec::new(),
            drain_limit: None,
            claim: String::new(),
        }
    }

    /// Settles the body and wakes everyone waiting for it.
    fn settle(&mut self, state: BodyState) {
        self.state = state;
        self.decoded = Vec::new();
        self.taken = 0;
        self.wake();
    }

    /// Wakes everyone waiting for the body, to look at it again.
    fn wake(&mut self) {
        for waiter in self.waiters.drain(..) {
            waiter.wake();
        }
    }

    /// The next piece of a stream: up to `size` of what is decoded and not yet
    /// taken, or `None` when nothing is. A copy — the piece is the reader's, and
    /// the buffer it came out of is reused.
    fn take_piece(&mut self, size: usize) -> Option<Bytes> {
        let available = self.decoded.len() - self.taken;
        if available == 0 {
            return None;
        }
        let end = self.taken + available.min(size.max(1));
        let piece = self.decoded[self.taken..end].to_vec();
        self.taken = end;
        if self.taken == self.decoded.len() {
            self.decoded.clear();
            self.taken = 0;
        }
        Some(Bytes::from_vec(piece))
    }
}

/// One accepted connection: the socket, what has been read, what is waiting to
/// be written, and the stage.
///
/// A connection carries one EXCHANGE at a time — a request and its answer — and,
/// when both sides let it persist, the next after it ([`Stage::Settling`]). The
/// per-exchange fields (`body`, `answered`, `persist`, `request_started`) are
/// reset between them.
struct Connection {
    stream: RefCell<TcpStream>,
    peer: String,
    policy: Rc<ServerPolicy>,
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
    /// The current request's body (M164).
    body: RefCell<Body>,
    /// Whether a response head has gone out for the current request — the
    /// application's, or a refusal's. One answer per request: a response that
    /// finds its request already answered writes nothing.
    answered: Cell<bool>,
    /// Whether the current exchange may leave the connection open for the next
    /// request: what the client asked for ([`Head::wants_to_persist`]), then
    /// what the response decided ([`Response::send_head`]). A refusal, a timeout
    /// and a stopping server clear it.
    persist: Cell<bool>,
    /// Which exchange the connection is on, counted from zero. A [`Request`]
    /// and a [`Response`] remember theirs, so a handler that outlives its own
    /// exchange cannot read the NEXT request's body or answer it.
    generation: Cell<u64>,
    /// When the current request began — its first byte, or the accept for a
    /// fresh connection — which both request deadlines count from. `None` while
    /// the connection is idle between requests.
    request_started: Cell<Option<Instant>>,
    /// When the connection went idle after an answer, which the keep-alive
    /// deadline counts from. `None` while a request is under way.
    idle_since: Cell<Option<Instant>>,
    /// A connection closing for a missed deadline closes by this moment whatever
    /// else it is waiting for — a client that is not reading its `408` included.
    close_by: Cell<Option<Instant>>,
    /// Whether the write half has been shut for a drain or a linger.
    write_shut: Cell<bool>,
    /// When a closing connection that may still have body bytes coming stops
    /// waiting for its client to read the answer (see [`LINGER`]).
    linger_until: Cell<Option<Instant>>,
}

impl Connection {
    fn new(stream: TcpStream, peer: String, policy: Rc<ServerPolicy>) -> Connection {
        Connection {
            stream: RefCell::new(stream),
            peer,
            policy,
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
            body: RefCell::new(Body::new(Framing::Empty)),
            answered: Cell::new(false),
            persist: Cell::new(false),
            generation: Cell::new(0),
            // A fresh connection is a request under way from its accept, as it
            // is to node: one that never sends a byte is bounded by the head's
            // deadline, not left open forever.
            request_started: Cell::new(Some(Instant::now())),
            idle_since: Cell::new(None),
            close_by: Cell::new(None),
            write_shut: Cell::new(false),
            linger_until: Cell::new(None),
        }
    }

    /// Whether the connection should take bytes off the socket this turn: while
    /// it reads a head, while it is upgraded, and while a body is being read or
    /// drained — never for a body nobody asked for, which stays in the kernel.
    ///
    /// A connection reading a head reads nothing while an earlier answer is
    /// still queued: a client that pipelines requests without reading the
    /// answers stalls in the kernel instead of growing this outbox.
    fn wants_bytes(&self) -> bool {
        match self.stage.get() {
            Stage::Reading => self.outbox.borrow().is_empty(),
            Stage::Upgraded => true,
            Stage::Dispatched | Stage::Settling | Stage::Closing => {
                let body = self.body.borrow();
                match body.state {
                    BodyState::Reading { .. } | BodyState::Draining { .. } => true,
                    // A stream reads only for a reader waiting with nothing to
                    // take (M176): between reads its body stays in the kernel.
                    BodyState::Streaming { ended: false, .. } => !body.waiters.is_empty(),
                    _ => false,
                }
            }
            Stage::Closed => false,
        }
    }

    /// Moves a body being read — whole, or as a stream — forward with what the
    /// inbox holds. `true` if anything changed.
    fn advance_body(&self, hung_up: bool) -> bool {
        let mut body = self.body.borrow_mut();
        let (limit, streaming) = match body.state {
            BodyState::Reading { limit } => (limit, false),
            BodyState::Streaming {
                limit,
                ended: false,
            } => (limit, true),
            _ => return false,
        };
        let mut inbox = self.inbox.borrow_mut();
        let before = inbox.len();
        let decoded_before = body.decoded.len();
        let outcome = match body.framing {
            Framing::Empty => Decoded::Done,
            Framing::Length(length) => {
                let taken = (length - body.received).min(inbox.len());
                body.decoded.extend(inbox.drain(..taken));
                if body.received + taken == length {
                    Decoded::Done
                } else {
                    Decoded::More
                }
            }
            Framing::Chunked => {
                // The decoder refuses a chunk that would take what `decoded`
                // holds past the limit it is given; the cap counts what the
                // body has PRODUCED, some of which a stream has already handed
                // out, so the limit it is given is the cap less those.
                let handed_out = body.received - body.decoded.len();
                let Body {
                    chunks, decoded, ..
                } = &mut *body;
                chunks.feed(&mut inbox, decoded, limit - handed_out)
            }
        };
        let grew = body.decoded.len() != decoded_before;
        body.received += body.decoded.len() - decoded_before;
        let progressed = inbox.len() != before;
        drop(inbox);
        match outcome {
            Decoded::Done if streaming => {
                body.state = BodyState::Streaming { limit, ended: true };
                body.wake();
                true
            }
            Decoded::Done => {
                let bytes = Bytes::from_vec(std::mem::take(&mut body.decoded));
                body.settle(BodyState::Read(bytes));
                true
            }
            // The client went away with the body half sent: the read ends as a
            // cancellation, and so does the connection — nobody is left to
            // answer, and a connection left `Dispatched` would never close (nor
            // let a graceful stop finish).
            Decoded::More if hung_up => {
                body.settle(BodyState::Gone);
                drop(body);
                self.close();
                true
            }
            Decoded::More => {
                if grew {
                    body.wake();
                }
                progressed
            }
            Decoded::OverLimit => {
                body.settle(BodyState::Refused);
                drop(body);
                self.refuse_body(limit);
                true
            }
            Decoded::Malformed => {
                body.settle(BodyState::Refused);
                drop(body);
                if !self.answered.replace(true) {
                    refuse_protocol(self, 400, "Bad Request");
                } else {
                    self.stage.set(Stage::Closing);
                }
                true
            }
        }
    }

    /// The over-the-cap answer (M164): `413` with the sentence node's twin
    /// sends, and the connection closing without the rest of the body read.
    fn refuse_body(&self, limit: usize) {
        self.refuse_with(
            413,
            &format!("the request body is larger than {limit} bytes"),
        );
    }

    /// A refusal with a sentence (M164's `413`, and M176's `400` and `500` that
    /// `std::http` answers through [`refuse`]): `status` with `message` as a
    /// plain-text body, and the connection closing without the rest of the body
    /// read. A response already under way cannot become a refusal, so it is cut
    /// instead.
    ///
    /// The head is node's for the same refusal, field for field and in node's
    /// order — the two fields `std::http` sets, then the length node computes —
    /// less node's `Date`.
    fn refuse_with(&self, status: u16, message: &str) {
        self.persist.set(false);
        if self.answered.replace(true) {
            self.stage.set(Stage::Closing);
            return;
        }
        let head = format!(
            "HTTP/1.1 {status} {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\
             Content-Length: {}\r\n\r\n",
            reason(status),
            message.len()
        );
        self.enqueue(head.as_bytes());
        self.enqueue(message.as_bytes());
        self.stage.set(Stage::Closing);
    }

    /// The answer has ended: the connection reads the next request, once the
    /// body has been read or discarded, when the exchange persists — and closes
    /// (draining or lingering first, [`Server::lingers`]) when it does not.
    ///
    /// Only a body whose end is KNOWN without reading it through a decoder can
    /// be discarded on the way to the next request: one read whole, an empty one,
    /// a stream that reached its end, or a declared length within what a drain
    /// may spend (the cap `std::http` remembered, or — for a caller that never
    /// asked — what has already arrived). Anything else is the M164 close.
    fn finish_exchange(&self) {
        if !self.persist.get() || self.policy.stopping.get() {
            self.stage.set(Stage::Closing);
            return;
        }
        let settles = {
            let mut body = self.body.borrow_mut();
            let unread = matches!(body.state, BodyState::Unread | BodyState::Abandoned);
            match (body.framing, &body.state) {
                (Framing::Empty, _)
                | (_, BodyState::Read(_))
                | (_, BodyState::Streaming { ended: true, .. })
                | (_, BodyState::Draining { .. }) => true,
                (Framing::Length(length), _) if unread => {
                    let remaining = length - body.received;
                    let drainable = match body.drain_limit {
                        Some(limit) => length <= limit,
                        None => self.inbox.borrow().len() >= remaining,
                    };
                    if drainable {
                        body.state = BodyState::Draining { remaining };
                    }
                    drainable
                }
                _ => false,
            }
        };
        self.stage.set(if settles {
            Stage::Settling
        } else {
            Stage::Closing
        });
    }

    /// The exchange is over and the connection persists: back to reading a
    /// request. What the client already sent past the last body — a PIPELINED
    /// request — is in the inbox, and is the next request's start.
    fn next_exchange(&self) {
        if self.policy.stopping.get() {
            self.stage.set(Stage::Closing);
            return;
        }
        *self.body.borrow_mut() = Body::new(Framing::Empty);
        self.answered.set(false);
        self.persist.set(false);
        self.generation.set(self.generation.get() + 1);
        if self.inbox.borrow().is_empty() {
            self.request_started.set(None);
            self.idle_since.set(Some(Instant::now()));
        } else {
            self.request_started.set(Some(Instant::now()));
            self.idle_since.set(None);
        }
        self.stage.set(Stage::Reading);
    }

    /// Whether the current request has arrived whole — its head, and its body
    /// read, discarded or empty — which is where `requestTimeout` stops counting.
    fn request_received(&self) -> bool {
        match self.stage.get() {
            Stage::Reading => false,
            Stage::Dispatched | Stage::Settling => {
                let body = self.body.borrow();
                match body.state {
                    _ if body.framing == Framing::Empty => true,
                    BodyState::Unread
                    | BodyState::Reading { .. }
                    | BodyState::Streaming { ended: false, .. }
                    | BodyState::Draining { .. } => false,
                    _ => true,
                }
            }
            Stage::Closing | Stage::Upgraded | Stage::Closed => true,
        }
    }

    /// A request missed its deadline (node's `onRequestTimeout`): answered
    /// `408 Request Timeout` when nothing has answered it yet, a body read in
    /// flight ended as a cancellation, and the connection closed within
    /// [`LINGER`] whatever it is still waiting for.
    fn time_out(&self) {
        {
            let mut body = self.body.borrow_mut();
            if matches!(
                body.state,
                BodyState::Unread
                    | BodyState::Reading { .. }
                    | BodyState::Streaming { .. }
                    | BodyState::Draining { .. }
            ) && body.framing != Framing::Empty
            {
                body.settle(BodyState::Refused);
            }
        }
        self.persist.set(false);
        if !self.answered.replace(true) {
            self.enqueue(REQUEST_TIMEOUT_RESPONSE);
        }
        self.stage.set(Stage::Closing);
        self.close_by.set(Some(Instant::now() + LINGER));
    }

    /// Discards what a drain is owed out of the inbox. `true` once the drain is
    /// over — everything owed taken, or the peer gone.
    fn advance_drain(&self, hung_up: bool) -> bool {
        let mut body = self.body.borrow_mut();
        let BodyState::Draining { remaining } = body.state else {
            return true;
        };
        let mut inbox = self.inbox.borrow_mut();
        let taken = remaining.min(inbox.len());
        inbox.drain(..taken);
        let left = remaining - taken;
        body.state = BodyState::Draining { remaining: left };
        left == 0 || hung_up
    }

    fn touch(&self) {
        self.last_activity.set(Instant::now());
        self.timed_out.set(false);
    }

    /// Reads whatever has arrived, or `budget` bytes of it — a body is read a
    /// bounded slice per turn and decoded in between, so a body over its cap is
    /// refused with at most a slice of it in memory. Answers `(progressed, the
    /// peer hung up)`.
    fn read_available(&self, budget: usize) -> (bool, bool) {
        let mut progressed = false;
        let mut chunk = [0u8; READ_CHUNK];
        let mut taken = 0;
        loop {
            if taken >= budget {
                return (progressed, false);
            }
            let read = self.stream.borrow_mut().read(&mut chunk);
            match read {
                Ok(0) => return (progressed, true),
                Ok(count) => {
                    if self.declined_body() {
                        REFUSED_BODY_READ.fetch_add(count, Ordering::Relaxed);
                    }
                    self.inbox.borrow_mut().extend_from_slice(&chunk[..count]);
                    taken += count;
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

    /// Whether the connection has decided not to read its body — refused it
    /// over the cap, or lingers with it unread — so any byte it reads now is
    /// one [`refused_body_census`] counts.
    fn declined_body(&self) -> bool {
        matches!(self.body.borrow().state, BodyState::Refused) || self.linger_until.get().is_some()
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
        {
            // A read still waiting ends: the body will never arrive now.
            let mut body = self.body.borrow_mut();
            if matches!(
                body.state,
                BodyState::Unread
                    | BodyState::Reading { .. }
                    | BodyState::Streaming { ended: false, .. }
            ) {
                body.settle(BodyState::Gone);
            }
        }
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
    /// property `std::rpc::server` arms a greeting bound with.
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

/// `NodeRequest` — one inbound request: its head, and the connection its body
/// is still on (M164: [`read_body`] takes it when the handler asks).
#[derive(Clone)]
pub struct Request(Rc<RequestBody>);

struct RequestBody {
    method: String,
    target: String,
    fields: Vec<(String, String)>,
    connection: Rc<Connection>,
    /// The exchange this request is ([`Connection::generation`]).
    generation: u64,
}

impl Request {
    /// Whether the connection has moved on to a later request: this one's body
    /// is over, and whatever is on the connection now is not its to read.
    fn stale(&self) -> bool {
        self.0.connection.generation.get() != self.0.generation
    }

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

    /// `__http_body_left_unread(request, limit)` — whether answering now leaves
    /// a body the connection should not drain: unread, and declared over
    /// `limit` or chunked. A body within `limit` is drained after the response
    /// instead (`limit` is remembered as the drain's bound), and a persistent
    /// connection goes on to its next request once the drain is done; a body
    /// left unread past it closes the connection, with the `Connection: close`
    /// `std::http` sets on its answer.
    ///
    /// A STREAM the handler left unfinished (M176) is the same body with part
    /// of it read: what the stream decoded and nobody took is let go here, and
    /// the rest is drained or not by the same rule.
    pub fn body_left_unread(&self, limit: usize) -> bool {
        if self.stale() {
            return false;
        }
        let mut body = self.0.connection.body.borrow_mut();
        match body.state {
            BodyState::Unread => {}
            // A reader that outlived the handler looks again, and finds the
            // body the connection's now.
            BodyState::Streaming { ended: false, .. } => body.settle(BodyState::Abandoned),
            _ => return false,
        }
        match body.framing {
            Framing::Empty => false,
            Framing::Length(length) if length <= limit => {
                body.drain_limit = Some(limit);
                false
            }
            Framing::Length(_) | Framing::Chunked => true,
        }
    }

    /// `request.socket` — the connection this request arrived on, as the
    /// [`Socket`] a protocol upgrade hands over (F163). `std::http` reads only
    /// its `remote_address_raw` through it (`Request::peer_address`); a write
    /// through it would land in the HTTP exchange's own byte stream, which is
    /// why std keeps the binding private.
    pub fn socket(&self) -> Socket {
        Socket(Rc::clone(&self.0.connection))
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

/// What [`refused_body_census`] reads: body bytes taken off connections after
/// they declined the body.
static REFUSED_BODY_READ: AtomicUsize = AtomicUsize::new(0);

/// `__http_refused_body_census()` — how many body bytes this process's server
/// read off its connections after deciding not to read them (B687; `std::http`'s
/// `refused_body_census`). Zero is M164's contract: a declined body stays in the
/// kernel. An instrument — it counts, it decides nothing.
pub fn refused_body_census() -> usize {
    REFUSED_BODY_READ.load(Ordering::Relaxed)
}

/// `__http_read_body(request, response, limit)` — the request's body, read on
/// the FIRST call and answered from there after (M164; `std::http`'s
/// `Request::bytes`). See the module comment for the cap; the response handle is
/// the JS twin's (node's `413` goes out on it), and here the connection answers
/// for itself.
pub fn read_body(request: Request, _response: Response, limit: usize) -> BodyRead {
    BodyRead { request, limit }
}

/// The future [`read_body`] returns. Public because emitted code's TYPE
/// inference names it, never its text.
pub struct BodyRead {
    request: Request,
    limit: usize,
}

impl Future for BodyRead {
    type Output = Bytes;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Bytes> {
        // A request whose exchange is over has no body left to read: the
        // connection's is the next request's.
        if self.request.stale() {
            raise_cancelled();
        }
        let connection = &self.request.0.connection;
        let starts = {
            let body = connection.body.borrow();
            match &body.state {
                BodyState::Read(bytes) => return Poll::Ready(bytes.clone()),
                BodyState::Unread => true,
                BodyState::Reading { .. } => false,
                BodyState::Streaming { .. }
                | BodyState::Abandoned
                | BodyState::Draining { .. }
                | BodyState::Refused
                | BodyState::Gone => {
                    drop(body);
                    raise_cancelled()
                }
            }
        };
        if starts {
            let mut body = connection.body.borrow_mut();
            if connection.answered.get() {
                // Read after the response went out: the body is the drain's now,
                // or already gone with the connection.
                body.settle(BodyState::Gone);
                drop(body);
                raise_cancelled();
            }
            if let Framing::Length(length) = body.framing
                && length > self.limit
            {
                // Over the cap by its own declaration: refused before a byte of
                // it is read.
                body.settle(BodyState::Refused);
                drop(body);
                connection.refuse_body(self.limit);
                raise_cancelled();
            }
            body.state = BodyState::Reading { limit: self.limit };
            drop(body);
            // Whatever arrived with the head is taken now; the poll takes the
            // rest as it comes.
            connection.advance_body(false);
            match &connection.body.borrow().state {
                BodyState::Read(bytes) => return Poll::Ready(bytes.clone()),
                BodyState::Reading { .. } => {}
                _ => raise_cancelled(),
            }
        }
        if let Some(parked) = Parked::current() {
            connection.body.borrow_mut().waiters.push(parked);
        }
        Poll::Pending
    }
}

/// `__http_claim_body(request, how)` — records how the handler reads the body
/// (`"whole"` or `"stream"`, M176's one-way-of-reading rule) when it has not
/// been claimed yet, and answers the earlier claim (`""` for none). `std::http`'s
/// `claim_read` refuses the request when the two differ.
pub fn claim_body(request: &Request, how: &str) -> Str {
    if request.stale() {
        return str_new("");
    }
    let mut body = request.0.connection.body.borrow_mut();
    let earlier = body.claim.clone();
    if earlier.is_empty() {
        body.claim = how.to_string();
    }
    str_new(&earlier)
}

/// `__http_refuse(request, response, status, message)` — `std::http`'s refusal
/// of a request on the server's side (M176/M177): `status` with `message` as
/// its body and the connection closing without the rest of the body read (a
/// response already under way is cut), then the caller ends as a cancellation,
/// exactly as an over-the-cap read does. A `5xx` is the server's own fault and
/// is written to stderr too, which is node's twin's `console.error`.
///
/// Typed `()` rather than `!`, although it never returns: the emitted code
/// calls it as the `void` extern it is declared as, and code after the call is
/// ordinary code there (unreachable code warns).
pub fn refuse(request: &Request, _response: &Response, status: i32, message: &str) {
    let connection = &request.0.connection;
    if status >= 500 {
        eprintln!("std::http: {message}");
    }
    if request.stale() {
        raise_cancelled();
    }
    {
        let mut body = connection.body.borrow_mut();
        if !matches!(body.state, BodyState::Gone) {
            body.settle(BodyState::Refused);
        }
    }
    connection.refuse_with(status.clamp(100, 599) as u16, message);
    raise_cancelled()
}

/// `__http_body_next(request, response, limit, size)` — the next piece of the
/// request body as a stream (M176; `std::http`'s `BodyReader::next`): at most
/// `size` bytes, `None` at the end. See [`BodyPiece`].
pub fn body_next(request: Request, _response: Response, limit: usize, size: usize) -> BodyPiece {
    BodyPiece {
        request,
        limit,
        size,
    }
}

/// The future [`body_next`] returns. Public because emitted code's TYPE
/// inference names it, never its text.
///
/// The first piece STARTS the stream — the cap's declared-length check, as the
/// whole read's, then whatever arrived with the head — and every piece is
/// answered from what is decoded and not yet taken. Only when nothing is does
/// the reader park, and only a parked reader makes the connection read
/// ([`Connection::wants_bytes`]): a [`BODY_SLICE`] a turn, decoded, the readers
/// woken. So the body is taken off the wire as fast as it is read and no faster,
/// and what is in memory is a slice, not the upload. A chunked body is counted
/// against the cap as it is decoded and refused at the chunk that crosses it,
/// however much of it the readers have already had.
pub struct BodyPiece {
    request: Request,
    limit: usize,
    size: usize,
}

impl Future for BodyPiece {
    type Output = Option<Bytes>;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Bytes>> {
        if self.request.stale() {
            raise_cancelled();
        }
        let connection = &self.request.0.connection;
        let starts = matches!(connection.body.borrow().state, BodyState::Unread);
        if starts {
            let mut body = connection.body.borrow_mut();
            if connection.answered.get() {
                // Read after the response went out: the body is the drain's now,
                // or already gone with the connection.
                body.settle(BodyState::Gone);
                drop(body);
                raise_cancelled();
            }
            if let Framing::Length(length) = body.framing
                && length > self.limit
            {
                body.settle(BodyState::Refused);
                drop(body);
                connection.refuse_body(self.limit);
                raise_cancelled();
            }
            body.state = BodyState::Streaming {
                limit: self.limit,
                ended: false,
            };
            drop(body);
            connection.advance_body(false);
        }
        let mut body = connection.body.borrow_mut();
        let BodyState::Streaming { ended, .. } = body.state else {
            // Refused, gone, drained — or read whole, which `std::http`'s
            // one-way-of-reading claim refuses before a reader can get here.
            drop(body);
            raise_cancelled()
        };
        if let Some(piece) = body.take_piece(self.size) {
            return Poll::Ready(Some(piece));
        }
        if ended {
            return Poll::Ready(None);
        }
        if let Some(parked) = Parked::current() {
            body.waiters.push(parked);
        }
        Poll::Pending
    }
}

/// `__http_find(haystack, needle, from)` — where `needle` next occurs in
/// `haystack` at or after `from` (M177's delimiter search; node's
/// `Buffer.indexOf`). An empty needle is found at `from`, as node's is when
/// `from` is within the haystack.
pub fn find_bytes(haystack: &Bytes, needle: &Bytes, from: usize) -> Option<usize> {
    let haystack = haystack.as_slice();
    let needle = needle.as_slice();
    if from > haystack.len() {
        return None;
    }
    if needle.is_empty() {
        return Some(from);
    }
    let first = needle[0];
    let mut index = from;
    while index + needle.len() <= haystack.len() {
        let at = index
            + haystack[index..=haystack.len() - needle.len()]
                .iter()
                .position(|byte| *byte == first)?;
        if haystack[at..at + needle.len()] == *needle {
            return Some(at);
        }
        index = at + 1;
    }
    None
}

// --------------------------------------------------------------- response ---

/// `NodeResponse` — the write half of one exchange.
#[derive(Clone)]
pub struct Response(Rc<ResponseBody>);

struct ResponseBody {
    connection: Rc<Connection>,
    /// The exchange this response answers ([`Connection::generation`]).
    generation: u64,
    status: Cell<u16>,
    headers: RefCell<Vec<(String, String)>>,
    /// Whether the status line and headers have gone out. A `setHeader` after
    /// that point is dropped, as node's is (it throws there; dropping is the
    /// conservative half of the divergence and is written down).
    head_sent: Cell<bool>,
    ended: Cell<bool>,
    /// Whether the request line said `HTTP/1.1` — the framing a response with
    /// no declared length can use, and the default the connection persists by.
    http11: bool,
    /// Whether the request was a `HEAD`: answered with every field its `GET`
    /// would carry, and no body (RFC 9110 §9.3.2).
    head_request: bool,
    /// Whether the body goes out `Transfer-Encoding: chunked` — a stream on an
    /// HTTP/1.1 exchange, as node's goes out.
    chunked: Cell<bool>,
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

    /// A field the PROGRAM set, by name, ignoring case.
    fn program_field(&self, wanted: &str) -> Option<String> {
        self.0
            .headers
            .borrow()
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
            .map(|(_, value)| value.clone())
    }

    /// Whether this response may still write: its own exchange is the
    /// connection's, and it has not ended. A response is over once it ends, and
    /// a connection moves on to the next request only after that — so a write
    /// to a stale response finds it ended; the generation is the second lock.
    fn open(&self) -> bool {
        !self.0.ended.get() && self.0.connection.generation.get() == self.0.generation
    }

    /// Writes the status line and headers if they have not gone out yet.
    ///
    /// **Node's head, field for field and in node's order, less its `Date`**
    /// (F151, Order 58's S3/S4): the status line with node's reason phrase, the
    /// fields the PROGRAM set in the order it set them, then what node adds —
    /// `Connection` (unless the program set it) with `Keep-Alive: timeout=N`
    /// on a persistent exchange, then the framing: `Content-Length` for a
    /// buffered body, `Transfer-Encoding: chunked` for a stream on HTTP/1.1,
    /// and nothing for a response that carries no body, for a `HEAD`, or where
    /// the program declared the framing itself. A body on an HTTP/1.0 exchange
    /// is ended by the close, which is what node writes for one (no length).
    /// Node's `Date` changes every second and is the one field the wire
    /// differential leaves out.
    ///
    /// Here, too, the exchange decides whether the connection persists: the
    /// client asked ([`Head::wants_to_persist`]), the server is not stopping,
    /// the program did not say `close`, and the receiver can find the body's end
    /// without the close.
    fn send_head(&self, content_length: Option<usize>) {
        if self.0.head_sent.replace(true) {
            return;
        }
        let connection = &self.0.connection;
        // One answer per request: a body refused under this response has
        // already answered `413` (M164), and a request past its deadline `408`;
        // this one is dropped.
        if connection.answered.replace(true) {
            self.0.ended.set(true);
            return;
        }
        let status = self.0.status.get();
        let program_connection = self.program_field("connection");
        let program_length = self.program_field("content-length").is_some();
        let program_framing = program_length || self.program_field("transfer-encoding").is_some();
        let has_body = status_carries_a_body(status) && !self.0.head_request;
        let chunked = has_body && !program_framing && self.0.http11 && content_length.is_none();
        let declared = has_body && !program_framing && self.0.http11 && content_length.is_some();
        // The receiver finds the end without the close: no body, a length (the
        // program's or ours), or chunks.
        let delimited = !has_body || program_length || declared || chunked;
        let persists = connection.persist.get()
            && !connection.policy.stopping.get()
            && delimited
            && !program_connection
                .as_deref()
                .is_some_and(|value| has_token(value, "close"));
        connection.persist.set(persists);
        self.0.chunked.set(chunked);
        let mut head = format!("HTTP/1.1 {status} {}\r\n", reason(status));
        for (name, value) in self.0.headers.borrow().iter() {
            head.push_str(name);
            head.push_str(": ");
            head.push_str(value);
            head.push_str("\r\n");
        }
        if program_connection.is_none() {
            if persists {
                head.push_str("Connection: keep-alive\r\n");
                // Node announces whole seconds, rounded down, and none at all
                // when the timeout is off.
                if let Some(timeout) = connection.policy.keep_alive_timeout.get() {
                    head.push_str(&format!("Keep-Alive: timeout={}\r\n", timeout.as_secs()));
                }
            } else {
                head.push_str("Connection: close\r\n");
            }
        }
        if declared && let Some(length) = content_length {
            head.push_str(&format!("Content-Length: {length}\r\n"));
        }
        if chunked {
            head.push_str("Transfer-Encoding: chunked\r\n");
        }
        head.push_str("\r\n");
        connection.enqueue(head.as_bytes());
    }

    /// Body bytes, framed as the head declared: one chunk of a chunked body,
    /// or the bytes themselves. Nothing for a response that carries no body, a
    /// `HEAD`'s included, and no empty chunk — a zero-size chunk ENDS a chunked
    /// body, which is why node skips an empty write too.
    fn send_body(&self, bytes: &[u8]) {
        if bytes.is_empty() || self.0.head_request || !status_carries_a_body(self.0.status.get()) {
            return;
        }
        let connection = &self.0.connection;
        if self.0.chunked.get() {
            connection.enqueue(format!("{:x}\r\n", bytes.len()).as_bytes());
            connection.enqueue(bytes);
            connection.enqueue(b"\r\n");
        } else {
            connection.enqueue(bytes);
        }
    }

    /// `response.write(chunk)` — a chunk without ending the response, which is
    /// SSE's shape. The head goes out on the first chunk, with no
    /// `Content-Length`, because a stream has no length to declare: it is
    /// chunked on HTTP/1.1 and ended by the close on HTTP/1.0, as node's is. A
    /// status that carries no body sends its head and drops the chunk, as node
    /// does.
    pub fn write(&self, chunk: &str) {
        if !self.open() {
            return;
        }
        self.send_head(None);
        if self.0.ended.get() {
            return;
        }
        self.send_body(chunk.as_bytes());
    }

    /// `response.end(body)`.
    pub fn end(&self, body: &str) {
        self.end_bytes(&Bytes::from_vec(body.as_bytes().to_vec()));
    }

    /// `response.end(bytes)`.
    ///
    /// A `204`, a `304` and every `1xx` carry NO body (RFC 9110 §6.4.1), and node
    /// treats them so: no `Content-Length` of its own and any data handed to
    /// `end` ignored. A `304` answering `If-None-Match` is the case that reaches
    /// here — `std::http`'s `etag_response` — and `Content-Length: 0` on it would
    /// claim the representation is empty. A response that already streamed
    /// declared no length, so its body cannot gain one now: it goes out as the
    /// last chunk, then the chunked body's end.
    pub fn end_bytes(&self, body: &Bytes) {
        if !self.open() {
            return;
        }
        let length = if self.0.head_sent.get() || !status_carries_a_body(self.0.status.get()) {
            None
        } else {
            Some(body.len())
        };
        self.send_head(length);
        if self.0.ended.replace(true) {
            // The request was already answered (a refused body, a missed
            // deadline): nothing of this response goes out.
            return;
        }
        self.send_body(&body.as_slice());
        if self.0.chunked.get() {
            self.0.connection.enqueue(b"0\r\n\r\n");
        }
        self.0.connection.finish_exchange();
    }

    /// M179: the handler FAILED. A response whose head has not gone out answers
    /// `500` with `Connection: close` — what node's `__http_respond` writes — and
    /// one already streaming is cut off where it stands (`destroy`, the flush
    /// kept), so its reader sees it end short rather than wait for an end that
    /// will not come. An ended response is left alone.
    fn fail(&self) {
        if self.0.ended.get() {
            return;
        }
        if !self.0.head_sent.get() {
            self.set_status_code(500);
            self.set_header("Content-Type", "text/plain");
            self.set_header("Connection", "close");
            self.end("internal server error");
        } else {
            self.0.ended.set(true);
            self.0.connection.destroy();
        }
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

/// Whether a response with this status may carry a body — every status but
/// the `1xx` family, `204 No Content` and `304 Not Modified` (RFC 9110 §6.4.1).
fn status_carries_a_body(status: u16) -> bool {
    !((100..200).contains(&status) || status == 204 || status == 304)
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
///
/// The bytes are node's own for a request its parser refuses
/// (`badRequestResponse` and its siblings in `_http_server.js`): the status
/// line and `Connection: close`, nothing else — the close ends the message.
fn refuse_protocol(connection: &Connection, status: u16, phrase: &str) {
    let head = format!("HTTP/1.1 {status} {phrase}\r\nConnection: close\r\n\r\n");
    connection.persist.set(false);
    connection.answered.set(true);
    connection.enqueue(head.as_bytes());
    connection.stage.set(Stage::Closing);
}

// ------------------------------------------------------------ termination ---

// The termination request itself (F45) is the executor's since F157: what it
// ends is the program's event loop, not only its servers. A server answers it
// at its next poll ([`Server`]'s `terminate`) and holds the stop open until its
// last connection has ended ([`IoSource::holds_the_stop`]).

// ----------------------------------------------------------------- server ---

/// `__http_respond(response, body)` (M164 + M179) — one request's answer. A
/// cancellation travels on and settles the spawned task quietly, as before; a
/// PANIC first answers the request (`Response::fail`: `500`, or a started stream
/// cut off) and then travels on, so the task's unobserved-failure line reports
/// it ONCE — `unhandled task error (spawned in an http request handler):
/// panicked at <site>: <message>`, the line node's helper prints — and the
/// server goes on serving. Before, the response was left open and the client
/// waited until it gave up.
pub async fn respond(response: Response, body: Boxed<()>) {
    match caught(body).await {
        Ok(()) => {}
        Err(failure) => {
            if !matches!(failure, Failure::Cancelled) {
                response.fail();
            }
            raise_failure(failure);
        }
    }
}

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
    /// The timeouts and the stopping flag every connection reads.
    policy: Rc<ServerPolicy>,
    port: Cell<u16>,
    /// `server.close(callback)`: stop accepting, fire when the last connection
    /// has ended.
    closing: Cell<bool>,
    on_closed: RefCell<Option<Rc<dyn Fn()>>>,
    /// Whether this server has already answered a termination request (F45) —
    /// it answers each one once, at the first poll that sees it.
    terminated: Cell<bool>,
}

/// `http.createServer(handler)`.
pub fn create_server(handler: Handler) -> Server {
    Server(Rc::new(ServerBody {
        handler,
        upgrade: RefCell::new(None),
        listener: RefCell::new(None),
        connections: RefCell::new(Vec::new()),
        policy: Rc::new(ServerPolicy::node_defaults()),
        port: Cell::new(0),
        closing: Cell::new(false),
        on_closed: RefCell::new(None),
        terminated: Cell::new(false),
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
    /// `on_ready` runs DEFERRED, once the code that called `listen` has
    /// finished its turn — node emits `'listening'` from `process.nextTick`,
    /// and the difference was observable after all: `std::http`'s `start()`
    /// runs the app's `on_start` from this callback, so `server.start();
    /// print("main returned")` printed its two lines in the opposite order
    /// natively (found building F45's stop pin). It cannot race the first
    /// request: the socket is bound here, but a connection is only ACCEPTED in
    /// the loop's I/O poll, and the microtask queue drains before every one.
    pub fn listen(&self, port: i32, on_ready: Rc<dyn Fn()>) {
        self.listen_at("0.0.0.0", port, on_ready);
    }

    /// `server.listen(port, host, on_ready)` — bound to `host` alone
    /// (`ServerBuilder::host`, M168): `127.0.0.1` is reachable from this
    /// machine only, where [`Self::listen`] takes every interface.
    pub fn listen_on(&self, port: i32, host: &str, on_ready: Rc<dyn Fn()>) {
        self.listen_at(host, port, on_ready);
    }

    fn listen_at(&self, host: &str, port: i32, on_ready: Rc<dyn Fn()>) {
        let requested = port.clamp(0, 65535) as u16;
        let listener = match TcpListener::bind((host, requested)) {
            Ok(listener) => listener,
            Err(error) => crate::panic_with(&format!(
                "the server could not bind {host} port {requested}: {error}"
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
        crate::executor::queue_microtask(move || on_ready());
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

    /// `server.headersTimeout = millis` — how long a request's head may take to
    /// arrive, counted from its first byte (from the accept, on a fresh
    /// connection); `0` switches it off. 60 s unless set, node's default.
    pub fn set_headers_timeout(&self, millis: i32) {
        self.0
            .policy
            .headers_timeout
            .set(ServerPolicy::deadline_of(millis));
    }

    /// `server.requestTimeout = millis` — how long a whole request, its body
    /// included, may take to arrive; `0` switches it off. 300 s unless set,
    /// node's default.
    pub fn set_request_timeout(&self, millis: i32) {
        self.0
            .policy
            .request_timeout
            .set(ServerPolicy::deadline_of(millis));
    }

    /// `server.keepAliveTimeout = millis` — how long a connection may sit idle
    /// between requests (announced as `Keep-Alive: timeout=N`, closed one second
    /// past it, as node closes it); `0` switches the announcement and the close
    /// off. 5 s unless set, node's default.
    pub fn set_keep_alive_timeout(&self, millis: i32) {
        self.0
            .policy
            .keep_alive_timeout
            .set(ServerPolicy::deadline_of(millis));
    }

    /// `server.connectionsCheckingInterval = millis` — how often NODE sweeps its
    /// connections for a missed request deadline. Accepted and ignored: this
    /// server checks every deadline on every turn of the loop, which is the
    /// interval node's sweep approximates.
    pub fn set_connections_checking_interval(&self, millis: i32) {
        let _ = millis;
    }

    /// `server.close(on_closed)` — stop accepting and fire once the existing
    /// connections have ended.
    ///
    /// Node's `close()` closes the connections that are idle between requests
    /// at once (`closeIdleConnections`, node 19 on), and so does this one; a
    /// request under way is answered, with `Connection: close` when its answer
    /// has not started, and its connection closes after it.
    pub fn close(&self, on_closed: Rc<dyn Fn()>) {
        self.0.closing.set(true);
        self.0.policy.stopping.set(true);
        *self.0.listener.borrow_mut() = None;
        *self.0.on_closed.borrow_mut() = Some(on_closed);
        let connections: Vec<Rc<Connection>> = self.0.connections.borrow().clone();
        for connection in connections {
            if connection.stage.get() == Stage::Reading
                && connection.request_started.get().is_none()
            {
                connection.close();
            }
        }
    }

    /// The graceful stop a termination request asks for (F45): stop accepting,
    /// close every connection that is only WAITING — one still reading a
    /// request, and an upgraded socket, whose `"close"` subscribers run (an rpc
    /// connection releases what it holds, as a hang-up would make it) — and let
    /// a request already being answered finish. The loop drops the server once
    /// its last connection has ended, and once no server holds the stop open
    /// the program ends (F157: [`crate::executor::request_termination`]).
    ///
    /// `node:http`'s `close()` keeps an upgraded socket open until its peer
    /// leaves; a signal-driven stop cannot wait on a peer, so this one does not.
    /// A `close` callback a program registered through `stop()` still fires
    /// when the last connection has gone — the two stops compose.
    fn terminate(&self) {
        self.0.policy.stopping.set(true);
        *self.0.listener.borrow_mut() = None;
        let connections: Vec<Rc<Connection>> = self.0.connections.borrow().clone();
        for connection in connections {
            // A handler still waiting for a body is waiting on the client, not
            // answering: it ends with the connection (its read is cancelled).
            let waits_on_the_client = matches!(
                connection.body.borrow().state,
                BodyState::Reading { .. } | BodyState::Draining { .. }
            );
            if matches!(connection.stage.get(), Stage::Reading | Stage::Upgraded)
                || waits_on_the_client
            {
                connection.close();
            }
        }
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
                        .push(Rc::new(Connection::new(
                            stream,
                            peer.ip().to_string(),
                            Rc::clone(&self.0.policy),
                        )));
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
        // A body nobody asked for is not read (M164): it waits in the kernel,
        // and a client sending more than the socket buffers holds stalls.
        let (read, hung_up) = if connection.wants_bytes() {
            let budget = match connection.stage.get() {
                Stage::Dispatched | Stage::Settling | Stage::Closing => BODY_SLICE,
                // A head is at most `MAX_HEAD`, and what a fast client has
                // already sent after it is a body nobody has asked for yet:
                // read past the head by at most one slice, and leave the rest
                // in the kernel for the reads a handler makes (N192: with no
                // bound, a 4 MiB chunked upload already on the wire was fed
                // whole to the decoder at the handler's first `next`, and the
                // cap refused it with `413` before the handler took its ten
                // bytes - on Linux, whose loopback buffers hold megabytes;
                // Windows never handed that much in one turn).
                Stage::Reading => MAX_HEAD + BODY_SLICE,
                _ => usize::MAX,
            };
            connection.read_available(budget)
        } else {
            (false, false)
        };
        progressed |= read;
        if read && connection.request_started.get().is_none() {
            // The first byte of the next request on a kept-alive connection:
            // its deadlines start counting, the idle one stops.
            connection.request_started.set(Some(Instant::now()));
            connection.idle_since.set(None);
        }
        match connection.stage.get() {
            Stage::Reading => {
                // The next request waits for the last answer to leave (see
                // `wants_bytes`), which keeps pipelined answers in order and
                // their queue one response long.
                if connection.outbox.borrow().is_empty() && self.dispatch(connection) {
                    progressed = true;
                } else if hung_up {
                    // The peer went away mid-head, or between requests: nothing
                    // to answer.
                    connection.close();
                    progressed = true;
                }
            }
            Stage::Dispatched => {
                if connection.advance_body(hung_up) {
                    progressed = true;
                }
            }
            Stage::Settling => {
                if connection.advance_drain(hung_up) {
                    if hung_up {
                        connection.close();
                    } else {
                        connection.next_exchange();
                    }
                    progressed = true;
                } else if read {
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
        if self.enforce_deadlines(connection) {
            progressed = true;
        }
        let (written, dead) = connection.write_available();
        progressed |= written;
        if dead {
            connection.close();
            return true;
        }
        if connection
            .close_by
            .get()
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            connection.close();
            return true;
        }
        if connection.stage.get() == Stage::Closing && connection.outbox.borrow().is_empty() {
            if self.lingers(connection, hung_up) {
                progressed |= read;
            } else {
                connection.close();
                progressed = true;
            }
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

    /// What a connection that has answered does before it closes (M164).
    /// `true` while it is still at it.
    ///
    /// A body the handler left unread within its cap is DRAINED: the write half
    /// is shut — the client sees the response end — and what the body still owes
    /// is read and discarded, so the close finds nothing unread and is a FIN.
    /// A body that may still be arriving and is NOT drained — refused over the
    /// cap, or unread past it — makes the connection LINGER: the write half is
    /// shut and nothing more is read for [`LINGER`], so the client reads its
    /// answer before the close that follows resets the rest.
    ///
    /// A connection closing for a missed deadline lingers the same way, up to
    /// its `close_by`: the client may still be writing the request it was too
    /// slow with, and the `408` should reach it ahead of the reset.
    fn lingers(&self, connection: &Rc<Connection>, hung_up: bool) -> bool {
        if let Some(deadline) = connection.close_by.get() {
            if !connection.write_shut.replace(true) {
                let _ = connection.stream.borrow().shutdown(Shutdown::Write);
            }
            return !hung_up && Instant::now() < deadline;
        }
        let draining = {
            let mut body = connection.body.borrow_mut();
            // An unread body — or the unread rest of an unfinished stream —
            // within the cap is drained.
            let unread = matches!(body.state, BodyState::Unread | BodyState::Abandoned);
            if let (true, Framing::Length(length), Some(limit)) =
                (unread, body.framing, body.drain_limit)
                && length <= limit
            {
                body.state = BodyState::Draining {
                    remaining: length - body.received,
                };
            }
            match body.state {
                BodyState::Draining { .. } => true,
                // Refused, or a stream left unfinished past what a drain may
                // spend: the rest is never read.
                BodyState::Refused | BodyState::Abandoned => false,
                // Unread, and not all of it already off the wire.
                BodyState::Unread => match body.framing {
                    Framing::Empty => return false,
                    Framing::Length(length) if connection.inbox.borrow().len() >= length => {
                        return false;
                    }
                    Framing::Length(_) | Framing::Chunked => false,
                },
                _ => return false,
            }
        };
        if !connection.write_shut.replace(true) {
            let _ = connection.stream.borrow().shutdown(Shutdown::Write);
        }
        if draining {
            return !connection.advance_drain(hung_up);
        }
        let deadline = connection
            .linger_until
            .get()
            .unwrap_or_else(|| Instant::now() + LINGER);
        connection.linger_until.set(Some(deadline));
        Instant::now() < deadline
    }

    /// The connection's three deadlines (see the module comment), checked once
    /// per turn. `true` if one fired.
    ///
    /// A request still reading its head has the nearer of `headersTimeout` and
    /// `requestTimeout`; one whose head arrived has `requestTimeout` until its
    /// body has too; a connection idle between requests has `keepAliveTimeout`
    /// plus node's grace, counted once its last answer has left. A connection
    /// already closing, or upgraded, has none of them.
    fn enforce_deadlines(&self, connection: &Rc<Connection>) -> bool {
        let policy = &connection.policy;
        let now = Instant::now();
        let elapsed_since = |moment: Instant| now.saturating_duration_since(moment);
        match connection.stage.get() {
            Stage::Reading => {
                if let Some(started) = connection.request_started.get() {
                    if policy
                        .head_deadline()
                        .is_some_and(|deadline| elapsed_since(started) >= deadline)
                    {
                        connection.time_out();
                        return true;
                    }
                } else if let (Some(idle), Some(timeout)) =
                    (connection.idle_since.get(), policy.keep_alive_timeout.get())
                    && connection.outbox.borrow().is_empty()
                    && elapsed_since(idle) >= timeout + KEEP_ALIVE_GRACE
                {
                    connection.close();
                    return true;
                }
                false
            }
            Stage::Dispatched | Stage::Settling => {
                let expired = connection.request_started.get().is_some_and(|started| {
                    policy
                        .request_timeout
                        .get()
                        .is_some_and(|deadline| elapsed_since(started) >= deadline)
                });
                if expired && !connection.request_received() {
                    connection.time_out();
                    return true;
                }
                false
            }
            Stage::Closing | Stage::Upgraded | Stage::Closed => false,
        }
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
                refuse_protocol(connection, status, phrase);
                return true;
            }
        };
        let framing = match body_framing(&head) {
            Ok(framing) => framing,
            Err(HeadError::Refused(status, phrase)) => {
                refuse_protocol(connection, status, phrase);
                return true;
            }
            // `body_framing` never asks for more bytes.
            Err(HeadError::Incomplete) => return false,
        };
        let upgrade = asks_to_upgrade(&head);
        let persists = head.wants_to_persist();
        let http11 = head.http11;
        let head_request = head.method == "HEAD";
        // The handler runs on the HEAD (M164): what arrived after it stays in
        // the inbox as the body's first bytes, and the rest stays on the wire
        // until the handler asks for it.
        let after_head = connection.inbox.borrow_mut().split_off(head.length);
        connection.inbox.borrow_mut().clear();
        let request = Request(Rc::new(RequestBody {
            method: head.method,
            target: head.target,
            fields: head.fields,
            connection: Rc::clone(connection),
            generation: connection.generation.get(),
        }));
        if upgrade {
            let handler = self.0.upgrade.borrow().clone();
            match handler {
                Some(handler) => {
                    // The socket leaves the request loop: no deadline of the
                    // loop's applies to it (its own `setTimeout` may).
                    connection.request_started.set(None);
                    connection.idle_since.set(None);
                    connection.stage.set(Stage::Upgraded);
                    let socket = Socket(Rc::clone(connection));
                    // The upgrade's "head" is whatever arrived after the request
                    // head — node's third argument, and for a WebSocket handshake
                    // it is almost always empty.
                    handler(request, socket, Bytes::from_vec(after_head));
                }
                // Node destroys an unclaimed upgrade socket, and so does
                // `std::http`'s own documentation of this path.
                None => connection.close(),
            }
            return true;
        }
        *connection.inbox.borrow_mut() = after_head;
        *connection.body.borrow_mut() = Body::new(framing);
        connection.persist.set(persists);
        connection.stage.set(Stage::Dispatched);
        let response = Response(Rc::new(ResponseBody {
            connection: Rc::clone(connection),
            generation: connection.generation.get(),
            status: Cell::new(200),
            headers: RefCell::new(Vec::new()),
            head_sent: Cell::new(false),
            ended: Cell::new(false),
            http11,
            head_request,
            chunked: Cell::new(false),
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
        if termination_requested() && !self.0.terminated.replace(true) {
            self.terminate();
        }
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

    fn holds_the_stop(&self) -> bool {
        // F157: a server holds a termination's graceful stop open while it is
        // live — until its terminate has run and its last answer has ended.
        self.is_live()
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
    fn the_body_framing_is_read_from_the_head() {
        let framing = |text: &str| body_framing(&head_of(text).expect("parses"));
        assert_eq!(
            framing("POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\n"),
            Ok(Framing::Length(5))
        );
        assert_eq!(framing("GET / HTTP/1.1\r\n\r\n"), Ok(Framing::Empty));
        assert_eq!(
            framing("POST / HTTP/1.1\r\nContent-Length: 0\r\n\r\n"),
            Ok(Framing::Empty)
        );
        // M164: chunked is DECODED now, where it was refused with `411`.
        assert_eq!(
            framing("POST / HTTP/1.1\r\nTransfer-Encoding: Chunked\r\n\r\n"),
            Ok(Framing::Chunked)
        );
        // A coding this server cannot undo (RFC 9112 §6.1).
        assert_eq!(
            framing("POST / HTTP/1.1\r\nTransfer-Encoding: gzip, chunked\r\n\r\n"),
            Err(HeadError::Refused(501, "Not Implemented"))
        );
        // Both framings at once: how a request is smuggled past a proxy.
        assert_eq!(
            framing("POST / HTTP/1.1\r\nContent-Length: 5\r\nTransfer-Encoding: chunked\r\n\r\n"),
            Err(HeadError::Refused(400, "Bad Request"))
        );
        assert_eq!(
            framing("POST / HTTP/1.1\r\nContent-Length: ten\r\n\r\n"),
            Err(HeadError::Refused(400, "Bad Request"))
        );
        // No size is refused at the head any more: the cap is the
        // application's, applied when the body is read.
        assert_eq!(
            framing("POST / HTTP/1.1\r\nContent-Length: 1000000000\r\n\r\n"),
            Ok(Framing::Length(1_000_000_000))
        );
    }

    // -- the chunked decoder, per rule (RFC 9112 §7.1) --

    /// Feeds `wire` to a fresh decoder `step` bytes at a time, as a socket
    /// could deliver it, and answers the last outcome and what was decoded.
    fn decode(wire: &[u8], step: usize, limit: usize) -> (Decoded, Vec<u8>) {
        let mut decoder = ChunkDecoder::new();
        let mut inbox = Vec::new();
        let mut output = Vec::new();
        let mut outcome = Decoded::More;
        for piece in wire.chunks(step) {
            inbox.extend_from_slice(piece);
            outcome = decoder.feed(&mut inbox, &mut output, limit);
            if outcome != Decoded::More {
                break;
            }
        }
        (outcome, output)
    }

    #[test]
    fn a_chunked_body_decodes_however_the_wire_splits_it() {
        let wire = b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
        for step in [1, 2, 3, 7, wire.len()] {
            assert_eq!(
                decode(wire, step, 1024),
                (Decoded::Done, b"hello world".to_vec()),
                "split every {step} bytes"
            );
        }
    }

    #[test]
    fn chunk_extensions_trailers_and_bare_line_feeds_are_accepted() {
        let wire = b"A;name=value\r\n0123456789\r\n0\r\nX-Checksum: 1\r\n\r\n";
        assert_eq!(
            decode(wire, 4, 1024),
            (Decoded::Done, b"0123456789".to_vec())
        );
        assert_eq!(
            decode(b"3\nabc\n0\n\n", 1, 1024),
            (Decoded::Done, b"abc".to_vec())
        );
    }

    #[test]
    fn a_chunk_that_would_cross_the_cap_is_refused_at_its_size_line() {
        // The second chunk's size takes the body to 12 of a 10-byte cap: refused
        // with its data still unsent — the bytes after the size line never
        // arrive here, and the outcome does not wait for them.
        let (outcome, decoded) = decode(b"5\r\nhello\r\n7\r\n", 1, 10);
        assert_eq!(outcome, Decoded::OverLimit);
        assert_eq!(decoded, b"hello".to_vec());
        // Exactly the cap is within it.
        assert_eq!(
            decode(b"5\r\nhello\r\n5\r\nworld\r\n0\r\n\r\n", 5, 10),
            (Decoded::Done, b"helloworld".to_vec())
        );
        // A size too large for any count is refused, never wrapped.
        assert_eq!(
            decode(b"ffffffffffffffffffff\r\n", 64, usize::MAX).0,
            Decoded::Malformed
        );
        assert_eq!(
            decode(b"ffffffffffffffff\r\n", 64, 10).0,
            Decoded::OverLimit
        );
    }

    #[test]
    fn malformed_chunk_framing_is_refused() {
        for wire in [
            &b"zz\r\nhello\r\n"[..],
            &b"\r\n"[..],
            &b"5\r\nhelloXX0\r\n\r\n"[..],
            &b"-5\r\nhello\r\n"[..],
        ] {
            assert_eq!(
                decode(wire, 1, 1024).0,
                Decoded::Malformed,
                "{:?}",
                String::from_utf8_lossy(wire)
            );
        }
        // An unbounded size line is an unbounded allocation: refused once it
        // outgrows any size a chunk could need.
        let long = vec![b'1'; MAX_CHUNK_LINE + 2];
        assert_eq!(decode(&long, 512, usize::MAX).0, Decoded::Malformed);
    }

    /// M177's delimiter search answers what node's `Buffer.indexOf` does: the
    /// first occurrence at or after `from`, a match at the very end, a near
    /// miss skipped, nothing past the haystack, and an empty needle at `from`.
    #[test]
    fn find_bytes_answers_the_first_occurrence_at_or_after_from() {
        let bytes = |text: &str| Bytes::from_vec(text.as_bytes().to_vec());
        let haystack = bytes("ab\r\n--b\r\n--bound\r\n--boundary");
        let needle = bytes("\r\n--boundary");
        assert_eq!(find_bytes(&haystack, &needle, 0), Some(16));
        assert_eq!(find_bytes(&haystack, &needle, 16), Some(16));
        assert_eq!(find_bytes(&haystack, &needle, 17), None);
        assert_eq!(find_bytes(&haystack, &bytes("--b"), 3), Some(4));
        assert_eq!(find_bytes(&haystack, &bytes("--b"), 5), Some(9));
        assert_eq!(find_bytes(&bytes("short"), &needle, 0), None);
        assert_eq!(find_bytes(&haystack, &needle, 1000), None);
        assert_eq!(find_bytes(&haystack, &bytes(""), 3), Some(3));
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

    /// A `304` carries no body and no length of its own — node's treatment of
    /// every status RFC 9110 §6.4.1 says has no content, and what `std::http`'s
    /// `etag_response` answers a matching `If-None-Match` with. A
    /// `Content-Length: 0` there would claim the representation is empty.
    #[test]
    fn a_not_modified_answer_sends_no_length_and_no_body() {
        let answered = block_on(async move {
            let handler: Handler = Rc::new(move |_request: Request, response: Response| {
                crate::executor::pin_future(async move {
                    response.set_status_code(304);
                    response.set_header("ETag", "\"v1\"");
                    response.end("ignored");
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            let client = fetch(port, "GET /x HTTP/1.1\r\nHost: a\r\n\r\n");
            crate::executor::sleep(200, None).await;
            server.close(Rc::new(|| {}));
            client.join().expect("the client thread")
        });
        let (status, headers, body) = split(&answered);
        assert_eq!(status, "HTTP/1.1 304 Not Modified");
        assert!(
            headers.iter().any(|line| line == "ETag: \"v1\""),
            "{headers:?}"
        );
        assert!(
            !headers
                .iter()
                .any(|line| line.starts_with("Content-Length")),
            "a 304 declares no length: {headers:?}"
        );
        assert_eq!(body, "", "and carries no body");
    }

    #[test]
    fn a_post_body_reaches_the_handler_whole() {
        let answered = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                crate::executor::pin_future(async move {
                    let body = read_body(request.clone(), response.clone(), 1024).await;
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

    /// M164: a chunked request is decoded — it was refused with `411` — and
    /// the handler reads the content, never the framing.
    #[test]
    fn a_chunked_request_is_decoded_over_the_wire() {
        let answered = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                crate::executor::pin_future(async move {
                    let body = read_body(request.clone(), response.clone(), 1024).await;
                    response.end(&String::from_utf8_lossy(&body.as_slice()));
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            let client = fetch(
                port,
                "POST / HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n\
                 6\r\n world\r\n0\r\n\r\n",
            );
            crate::executor::sleep(200, None).await;
            server.close(Rc::new(|| {}));
            client.join().expect("the client thread")
        });
        let (status, _, body) = split(&answered);
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert_eq!(body, "hello world");
    }

    /// M164: a body declared over the cap the read is given is answered `413`
    /// before a byte of it is read — the client here never SENDS one, so a
    /// server that waited for it would answer nothing — and the read ends its
    /// task as a cancellation: the handler's code after it does not run, and
    /// the cancellation is not reported.
    #[test]
    fn a_body_declared_over_the_cap_is_refused_before_it_is_read() {
        let after_the_read = Rc::new(Cell::new(false));
        let reached = Rc::clone(&after_the_read);
        let answered = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                let reached = Rc::clone(&reached);
                crate::executor::pin_future(async move {
                    let _body = read_body(request.clone(), response.clone(), 10).await;
                    reached.set(true);
                    response.end("the handler must not go on");
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            let client = fetch(
                port,
                "POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 11\r\n\r\n",
            );
            crate::executor::sleep(200, None).await;
            server.close(Rc::new(|| {}));
            client.join().expect("the client thread")
        });
        let (status, headers, body) = split(&answered);
        assert_eq!(status, "HTTP/1.1 413 Payload Too Large");
        assert!(
            headers.iter().any(|line| line == "Connection: close"),
            "{headers:?}"
        );
        assert_eq!(body, "the request body is larger than 10 bytes");
        assert!(
            !after_the_read.get(),
            "the handler ends at the refused read"
        );
    }

    /// M164: a chunked body is counted as it arrives and refused at the chunk
    /// that crosses the cap — the client here never ends the body, so a server
    /// that read to the end before counting would answer nothing.
    #[test]
    fn a_chunked_body_over_the_cap_is_refused_as_it_arrives() {
        let answered = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                crate::executor::pin_future(async move {
                    let _body = read_body(request.clone(), response.clone(), 8).await;
                    response.end("the handler must not go on");
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            let client = fetch(
                port,
                "POST / HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n\
                 5\r\nworld\r\n",
            );
            crate::executor::sleep(200, None).await;
            server.close(Rc::new(|| {}));
            client.join().expect("the client thread")
        });
        let (status, _, body) = split(&answered);
        assert_eq!(status, "HTTP/1.1 413 Payload Too Large");
        assert_eq!(body, "the request body is larger than 8 bytes");
    }

    /// Polls the executor until `ready` holds — a LIVENESS loop over a fact
    /// the test's own sequencing establishes, never a sleep standing in for an
    /// ordering.
    async fn until(ready: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready() {
            assert!(Instant::now() < deadline, "the awaited state never came");
            crate::executor::sleep(1, None).await;
        }
    }

    /// M164's keep-alive rule, natively: a handler that answers WITHOUT reading
    /// a body within the cap leaves the body to a drain — the response goes out
    /// and the write half is shut, the connection stays to take what the body
    /// still owes and discard it, and closes once it has (a FIN, not the reset a
    /// close over unread bytes sends, which can cost the client its response).
    ///
    /// The order is the test's own: the client sends the head, reads the WHOLE
    /// response (its length is declared), and only then — told so over a
    /// channel — sends the body. Between the two the server must still hold the
    /// connection, draining.
    #[test]
    fn an_unread_body_within_the_cap_is_drained_before_the_close() {
        let (send_body, body_may_go) = std::sync::mpsc::channel::<()>();
        let (response_read, read_signal) = std::sync::mpsc::channel::<String>();
        let observed = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                crate::executor::pin_future(async move {
                    // `std::http`'s `answer`, for a body within its cap.
                    assert!(!request.body_left_unread(1024));
                    response.set_status_code(401);
                    response.end("sign in");
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            let client = std::thread::spawn(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                stream
                    .write_all(
                        b"POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 11\r\nConnection: close\r\n\r\n",
                    )
                    .expect("send the head");
                let mut response = Vec::new();
                let mut byte = [0u8; 1];
                while !String::from_utf8_lossy(&response).ends_with("sign in") {
                    stream.read_exact(&mut byte).expect("read the response");
                    response.push(byte[0]);
                }
                response_read
                    .send(String::from_utf8_lossy(&response).into_owned())
                    .expect("report the response");
                body_may_go.recv().expect("wait to send the body");
                stream.write_all(b"hello world").expect("send the body");
                let mut rest = Vec::new();
                // A drained close is a FIN: the read ENDS, it does not fail.
                stream.read_to_end(&mut rest).map(|_| rest.len())
            });
            let response = loop {
                if let Ok(response) = read_signal.try_recv() {
                    break response;
                }
                crate::executor::sleep(1, None).await;
            };
            let draining = matches!(
                server.0.connections.borrow().first().map(|connection| {
                    matches!(
                        connection.body.borrow().state,
                        BodyState::Draining { remaining: 11 }
                    )
                }),
                Some(true)
            );
            send_body.send(()).expect("let the body go");
            let probe = server.clone();
            until(move || probe.0.connections.borrow().is_empty()).await;
            server.close(Rc::new(|| {}));
            let rest = client
                .join()
                .expect("the client thread")
                .map_err(|error| error.to_string());
            (response, draining, rest)
        });
        let (response, draining, rest) = observed;
        assert!(
            response.starts_with("HTTP/1.1 401 Unauthorized"),
            "{response}"
        );
        assert!(
            draining,
            "with the response out, the connection waits for the body to discard"
        );
        assert_eq!(
            rest.ok(),
            Some(0),
            "the drained close ends the read cleanly"
        );
    }

    /// The other half of the rule: an unread body OVER the cap (or chunked) is
    /// not drained — the connection closes once the response is out, without
    /// waiting for a body the client never sends here.
    #[test]
    fn an_unread_body_over_the_cap_closes_once_the_response_is_out() {
        let answered = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                crate::executor::pin_future(async move {
                    let leaves_it = request.body_left_unread(1024);
                    if leaves_it {
                        response.set_header("Connection", "close");
                    }
                    response.set_status_code(401);
                    response.end(&format!("left unread: {leaves_it}"));
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            // The client reads to the close; a server that drained would wait
            // for 2048 bytes this client never sends, and never close.
            let client = Rc::new(RefCell::new(Some(fetch(
                port,
                "POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 2048\r\n\r\n",
            ))));
            let watched = Rc::clone(&client);
            until(move || {
                watched
                    .borrow()
                    .as_ref()
                    .is_some_and(|handle| handle.is_finished())
            })
            .await;
            let response = client
                .borrow_mut()
                .take()
                .expect("the client handle")
                .join()
                .expect("the client thread");
            server.close(Rc::new(|| {}));
            response
        });
        let (status, headers, body) = split(&answered);
        assert_eq!(status, "HTTP/1.1 401 Unauthorized");
        assert_eq!(body, "left unread: true");
        assert_eq!(
            headers
                .iter()
                .filter(|line| line.eq_ignore_ascii_case("connection: close"))
                .count(),
            1,
            "the program's `Connection: close` is not written twice: {headers:?}"
        );
    }

    /// A client that hangs up halfway through a body being read: the read
    /// ends (the handler's code after it does not run) and the connection is
    /// CLOSED — it was left `Dispatched` forever, holding its socket and keeping
    /// a graceful stop from finishing.
    #[test]
    fn a_client_gone_mid_body_ends_the_read_and_the_connection() {
        let after_the_read = Rc::new(Cell::new(false));
        let reached = Rc::clone(&after_the_read);
        let dispatched = Rc::new(Cell::new(false));
        let started = Rc::clone(&dispatched);
        let left = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                let reached = Rc::clone(&reached);
                let started = Rc::clone(&started);
                crate::executor::pin_future(async move {
                    started.set(true);
                    let _body = read_body(request.clone(), response.clone(), 1024).await;
                    reached.set(true);
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            let client = std::thread::spawn(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                stream
                    .write_all(b"POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 100\r\n\r\nhalf")
                    .expect("send half a request");
            });
            client.join().expect("the client thread");
            let seen = Rc::clone(&dispatched);
            until(move || seen.get()).await;
            let probe = server.clone();
            until(move || probe.0.connections.borrow().is_empty()).await;
            server.close(Rc::new(|| {}));
            server.0.connections.borrow().len()
        });
        assert_eq!(left, 0, "the connection closes with its client");
        assert!(!after_the_read.get(), "the handler ends at the read");
    }

    /// A stream has no length to declare, so its framing is the version's, as
    /// node's is: `Transfer-Encoding: chunked` on HTTP/1.1 — each `write` one
    /// chunk, an empty `end` the terminating zero-size chunk, the connection
    /// kept — and the close on HTTP/1.0, the bytes as written.
    #[test]
    fn a_streaming_response_is_chunked_on_http11_and_ended_by_the_close_on_http10() {
        let (chunked, plain) = block_on(async move {
            let handler: Handler = Rc::new(move |_request: Request, response: Response| {
                crate::executor::pin_future(async move {
                    response.set_header("Content-Type", "text/event-stream");
                    response.write("data: one\n\n");
                    crate::executor::sleep(10, None).await;
                    // An empty write is no chunk: a zero-size one would END the
                    // body.
                    response.write("");
                    response.write("data: two\n\n");
                    response.end("");
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            let chunked = fetch(port, "GET /events HTTP/1.1\r\nHost: a\r\n\r\n");
            let plain = fetch(port, "GET /events HTTP/1.0\r\n\r\n");
            crate::executor::sleep(300, None).await;
            // The HTTP/1.1 connection is idle and kept: `close` ends it, which
            // is what lets its client's read finish.
            server.close(Rc::new(|| {}));
            (
                chunked.join().expect("the client thread"),
                plain.join().expect("the client thread"),
            )
        });
        assert_eq!(
            chunked,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: keep-alive\r\n\
             Keep-Alive: timeout=5\r\nTransfer-Encoding: chunked\r\n\r\n\
             b\r\ndata: one\n\n\r\nb\r\ndata: two\n\n\r\n0\r\n\r\n"
        );
        assert_eq!(
            plain,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n\
             data: one\n\ndata: two\n\n"
        );
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
    /// [`fetch`] with a bounded read: a connection the server never closes
    /// answers `"<still open>"` after five seconds instead of hanging the test.
    fn fetch_bounded(port: u16, request: &str) -> std::thread::JoinHandle<String> {
        let request = request.to_string();
        std::thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("bound the read");
            stream.write_all(request.as_bytes()).expect("send");
            let mut response = String::new();
            match stream.read_to_string(&mut response) {
                Ok(_) => response,
                Err(_) => "<still open>".to_string(),
            }
        })
    }

    /// F45: the graceful stop a termination request makes. A connection only
    /// WAITING (connected, nothing sent) and an upgraded socket are closed —
    /// the socket's `"close"` subscribers run, which is how an rpc connection
    /// releases what it holds — while a request already being answered
    /// finishes, and then the server is no longer live, which is what lets the
    /// loop, and `main`, end. `terminate` is called directly: the process-wide
    /// request counter it answers would stop every server in this test binary.
    ///
    /// Ordering by gaps, not by luck (N116): the requests are in the server's
    /// hands 100 ms before the stop, and the in-flight handler answers 1,000 ms
    /// after it — ten times the gap, and far past any poll.
    #[test]
    fn a_termination_closes_the_waiting_and_lets_the_answering_finish() {
        let released = Rc::new(Cell::new(false));
        let observed = Rc::clone(&released);
        let (answered, idle, upgraded, live_after) = block_on(async move {
            let handler: Handler = Rc::new(move |_request: Request, response: Response| {
                crate::executor::pin_future(async move {
                    crate::executor::sleep(1000, None).await;
                    response.end("finished\n");
                })
            });
            let server = create_server(handler);
            let on_close = Rc::clone(&observed);
            server.on_upgrade(
                "upgrade",
                Rc::new(move |_request: Request, socket: Socket, _head: Bytes| {
                    let on_close = Rc::clone(&on_close);
                    socket.on_signal("close", Rc::new(move || on_close.set(true)));
                }),
            );
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            let answering = fetch_bounded(port, "GET /slow HTTP/1.1\r\nHost: a\r\n\r\n");
            let idle = fetch_bounded(port, "");
            let upgraded = fetch_bounded(
                port,
                "GET /ws HTTP/1.1\r\nHost: a\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n",
            );
            crate::executor::sleep(100, None).await;
            server.terminate();
            // The loop would drop the server once it is no longer live; here the
            // test keeps it and asks, after the in-flight answer has gone out.
            crate::executor::sleep(1500, None).await;
            let live_after = server.is_live();
            (
                answering.join().expect("the answering client"),
                idle.join().expect("the idle client"),
                upgraded.join().expect("the upgraded client"),
                live_after,
            )
        });
        let (status, _headers, body) = split(&answered);
        assert_eq!(status, "HTTP/1.1 200 OK", "the in-flight request finishes");
        assert_eq!(body, "finished\n");
        assert_eq!(idle, "", "a waiting connection is closed with nothing sent");
        assert_eq!(upgraded, "", "an upgraded socket is closed");
        assert!(
            released.get(),
            "the upgraded socket's `close` subscribers run"
        );
        assert!(
            !live_after,
            "a stopped server with no connections is not live"
        );
    }

    // -- persistent connections and their deadlines (F152, Order 58's S3) --

    /// Starts a server on a free port with `handler` and answers it and the
    /// port.
    fn serve(handler: Handler) -> (Server, u16) {
        let server = create_server(handler);
        server.listen(0, Rc::new(|| {}));
        let port = server.port() as u16;
        (server, port)
    }

    /// Lets the loop serve until `client` has finished, then closes the server
    /// and answers what the client did. The client's own ending is the
    /// ordering — never a sleep.
    async fn finish<T>(server: Server, client: std::thread::JoinHandle<T>) -> T {
        until(|| client.is_finished()).await;
        server.close(Rc::new(|| {}));
        client.join().expect("the client thread")
    }

    /// A handler answering `<method> <target>` as a buffered `200`.
    fn echo_target() -> Handler {
        Rc::new(move |request: Request, response: Response| {
            crate::executor::pin_future(async move {
                response.set_header("Content-Type", "text/plain");
                response.end(&format!("{} {}", request.method(), request.url()));
            })
        })
    }

    /// Reads ONE response off a connection that may stay open: the head, then
    /// the body by the head's own framing — its `Content-Length`, its chunks, or
    /// (with neither, on an answer that closes) the rest of the stream. A head
    /// that declares neither on a kept-alive connection has no body (`HEAD`,
    /// `204`, `304`). Answers the raw text; an empty string when the server
    /// closed before a head.
    fn read_response(stream: &mut TcpStream) -> String {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(0) | Err(_) => return String::from_utf8_lossy(&head).into_owned(),
                Ok(_) => head.push(byte[0]),
            }
        }
        let text = String::from_utf8_lossy(&head).into_owned();
        let lower = text.to_ascii_lowercase();
        let mut body = Vec::new();
        if let Some(line) = lower
            .lines()
            .find(|line| line.starts_with("content-length:"))
        {
            let length: usize = line["content-length:".len()..]
                .trim()
                .parse()
                .expect("a numeric length");
            body.resize(length, 0);
            stream.read_exact(&mut body).expect("read the body");
        } else if lower.contains("\r\ntransfer-encoding: chunked\r\n") {
            while !body.ends_with(b"\r\n0\r\n\r\n") && body != b"0\r\n\r\n" {
                stream.read_exact(&mut byte).expect("read a chunk");
                body.push(byte[0]);
            }
        } else if lower.contains("\r\nconnection: close\r\n") {
            stream.read_to_end(&mut body).expect("read to the close");
        }
        text + &String::from_utf8_lossy(&body)
    }

    /// Whether the server has closed `stream`: a read answers its end (or a
    /// reset) within `bound`, where an open connection would time the read out.
    fn closed_within(stream: &mut TcpStream, bound: Duration) -> bool {
        stream
            .set_read_timeout(Some(bound))
            .expect("bound the read");
        let mut byte = [0u8; 1];
        match stream.read(&mut byte) {
            Ok(0) => true,
            Ok(_) => false,
            Err(error) => !matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut),
        }
    }

    /// Three requests over ONE connection: the first two are kept (node's
    /// `Connection: keep-alive` and `Keep-Alive: timeout=5`, which is what the
    /// default 5 s announces), and the third, which says `Connection: close`,
    /// is answered `Connection: close` and the connection ends after it.
    #[test]
    fn a_kept_alive_connection_serves_requests_one_after_another() {
        let answers = block_on(async move {
            let (server, port) = serve(echo_target());
            let client = std::thread::spawn(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                let mut answers = Vec::new();
                for request in [
                    "GET /one HTTP/1.1\r\nHost: a\r\n\r\n",
                    "GET /two HTTP/1.1\r\nHost: a\r\n\r\n",
                    "GET /three HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n",
                ] {
                    stream.write_all(request.as_bytes()).expect("send");
                    answers.push(read_response(&mut stream));
                }
                let closed = closed_within(&mut stream, Duration::from_secs(5));
                (answers, closed)
            });
            finish(server, client).await
        });
        let (answers, closed) = answers;
        assert_eq!(
            answers,
            vec![
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: keep-alive\r\n\
                 Keep-Alive: timeout=5\r\nContent-Length: 8\r\n\r\nGET /one"
                    .to_string(),
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: keep-alive\r\n\
                 Keep-Alive: timeout=5\r\nContent-Length: 8\r\n\r\nGET /two"
                    .to_string(),
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\
                 Content-Length: 10\r\n\r\nGET /three"
                    .to_string(),
            ]
        );
        assert!(
            closed,
            "the connection ends after the answer that said close"
        );
    }

    /// The terms each exchange is answered on, as node answers them (measured
    /// against node 24): HTTP/1.0 closes — even when it asks to keep the
    /// connection, its body goes out with no length and the close ends it — a
    /// program's own `Connection: close` is honoured and not written twice, and
    /// a `HEAD` and a `204` carry neither a body nor a length and keep the
    /// connection.
    #[test]
    fn each_exchange_is_answered_on_nodes_connection_terms() {
        let answers = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                crate::executor::pin_future(async move {
                    let target = request.url().to_string();
                    response.set_header("Content-Type", "text/plain");
                    if target == "/close" {
                        response.set_header("Connection", "close");
                    }
                    if target == "/empty" {
                        response.set_status_code(204);
                    }
                    response.end("hi");
                })
            });
            let (server, port) = serve(handler);
            let client = std::thread::spawn(move || {
                [
                    "GET / HTTP/1.0\r\n\r\n",
                    "GET / HTTP/1.0\r\nConnection: keep-alive\r\n\r\n",
                    "GET /close HTTP/1.1\r\nHost: a\r\n\r\n",
                    "GET / HTTP/1.1\r\nHost: a\r\nConnection: Keep-Alive, Close\r\n\r\n",
                    "HEAD / HTTP/1.1\r\nHost: a\r\n\r\n",
                    "GET /empty HTTP/1.1\r\nHost: a\r\n\r\n",
                ]
                .iter()
                .map(|request| {
                    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                    stream.write_all(request.as_bytes()).expect("send");
                    read_response(&mut stream)
                })
                .collect::<Vec<String>>()
            });
            finish(server, client).await
        });
        let keep = "Connection: keep-alive\r\nKeep-Alive: timeout=5\r\n";
        assert_eq!(
            answers,
            vec![
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nhi"
                    .to_string(),
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nhi"
                    .to_string(),
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\
                 Content-Length: 2\r\n\r\nhi"
                    .to_string(),
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\
                 Content-Length: 2\r\n\r\nhi"
                    .to_string(),
                format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n{keep}\r\n"),
                format!("HTTP/1.1 204 No Content\r\nContent-Type: text/plain\r\n{keep}\r\n"),
            ]
        );
    }

    /// F151: the reason phrase is node's for every code — the old names node
    /// keeps (`413 Payload Too Large`, `422 Unprocessable Entity`,
    /// `418 I'm a Teapot`) and node's `unknown` for a code it does not list.
    #[test]
    fn reason_phrases_are_nodes() {
        assert_eq!(reason(413), "Payload Too Large");
        assert_eq!(reason(422), "Unprocessable Entity");
        assert_eq!(reason(418), "I'm a Teapot");
        assert_eq!(reason(408), "Request Timeout");
        assert_eq!(reason(299), "unknown");
        assert_eq!(reason(599), "unknown");
    }

    /// Requests a client pipelines — three in one write — are answered in the
    /// order they were sent, each handler starting only once the one before it
    /// has answered: the first sleeps, and the second still waits for it.
    #[test]
    fn pipelined_requests_are_answered_in_order() {
        let events = Rc::new(RefCell::new(Vec::<String>::new()));
        let log = Rc::clone(&events);
        let answered = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                let log = Rc::clone(&log);
                crate::executor::pin_future(async move {
                    let target = request.url().to_string();
                    log.borrow_mut().push(format!("start {target}"));
                    if target == "/slow" {
                        crate::executor::sleep(50, None).await;
                    }
                    log.borrow_mut().push(format!("end {target}"));
                    response.end(&target);
                })
            });
            let (server, port) = serve(handler);
            let client = std::thread::spawn(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                stream
                    .write_all(
                        b"GET /slow HTTP/1.1\r\nHost: a\r\n\r\n\
                          GET /fast HTTP/1.1\r\nHost: a\r\n\r\n\
                          GET /last HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n",
                    )
                    .expect("send all three");
                let mut everything = String::new();
                stream
                    .read_to_string(&mut everything)
                    .expect("read to the close");
                everything
            });
            finish(server, client).await
        });
        let bodies: Vec<&str> = answered
            .split("HTTP/1.1 200 OK")
            .skip(1)
            .map(|answer| answer.rsplit("\r\n\r\n").next().unwrap_or_default())
            .collect();
        assert_eq!(bodies, vec!["/slow", "/fast", "/last"], "{answered}");
        assert_eq!(
            events.borrow().clone(),
            vec![
                "start /slow",
                "end /slow",
                "start /fast",
                "end /fast",
                "start /last",
                "end /last"
            ]
        );
    }

    /// A slow-loris head: a client that trickles header lines and never ends
    /// the head is answered node's `408 Request Timeout` and closed at the
    /// head's deadline — counted from the request's first byte, so the trickle
    /// does not extend it — and so is a connection that never sends a byte.
    #[test]
    fn a_head_that_misses_its_deadline_is_answered_408_and_closed() {
        let (trickled, silent) = block_on(async move {
            let (server, port) = serve(echo_target());
            server.set_headers_timeout(300);
            let client = std::thread::spawn(move || {
                let trickle = std::thread::spawn(move || {
                    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                    let started = Instant::now();
                    stream
                        .write_all(b"GET / HTTP/1.1\r\n")
                        .expect("send the request line");
                    // A line every 25 ms until the answer comes — each wait is
                    // a read bounded at 25 ms, so the client stops sending at
                    // the answer and reads it before the server's reset.
                    stream
                        .set_read_timeout(Some(Duration::from_millis(25)))
                        .expect("bound the read");
                    let mut answer = Vec::new();
                    let mut chunk = [0u8; 256];
                    for line in 0..200 {
                        if let Ok(count) = stream.read(&mut chunk) {
                            answer.extend_from_slice(&chunk[..count]);
                            break;
                        }
                        stream
                            .write_all(format!("X-Line-{line}: 1\r\n").as_bytes())
                            .expect("trickle a line");
                    }
                    let elapsed = started.elapsed();
                    stream.set_read_timeout(None).expect("unbound the read");
                    let _ = stream.read_to_end(&mut answer);
                    (String::from_utf8_lossy(&answer).into_owned(), elapsed)
                });
                let mut silent = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                let started = Instant::now();
                let mut answer = String::new();
                silent
                    .read_to_string(&mut answer)
                    .expect("read to the close");
                (
                    trickle.join().expect("the trickling client"),
                    (answer, started.elapsed()),
                )
            });
            finish(server, client).await
        });
        for (name, (answer, elapsed)) in [("trickled", trickled), ("silent", silent)] {
            assert_eq!(
                answer, "HTTP/1.1 408 Request Timeout\r\nConnection: close\r\n\r\n",
                "{name}"
            );
            assert!(
                elapsed >= Duration::from_millis(250) && elapsed < Duration::from_secs(3),
                "{name}: answered at the head's deadline, not before or long after: {elapsed:?}"
            );
        }
    }

    /// A body that stalls while the handler reads it: the request misses
    /// `requestTimeout`, is answered `408` (nothing has answered it), the read
    /// ends as a cancellation — the handler's code after it never runs — and
    /// the connection closes.
    #[test]
    fn a_stalled_body_misses_the_request_deadline_and_cancels_the_read() {
        let after_the_read = Rc::new(Cell::new(false));
        let observed = Rc::clone(&after_the_read);
        let (answer, elapsed) = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                let observed = Rc::clone(&observed);
                crate::executor::pin_future(async move {
                    let _ = read_body(request.clone(), response.clone(), 1024).await;
                    observed.set(true);
                    response.end("read it all");
                })
            });
            let (server, port) = serve(handler);
            server.set_request_timeout(300);
            let client = std::thread::spawn(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                let started = Instant::now();
                stream
                    .write_all(b"POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 10\r\n\r\nabc")
                    .expect("send three bytes of ten");
                let mut answer = String::new();
                stream
                    .read_to_string(&mut answer)
                    .expect("read to the close");
                (answer, started.elapsed())
            });
            finish(server, client).await
        });
        assert_eq!(
            answer,
            "HTTP/1.1 408 Request Timeout\r\nConnection: close\r\n\r\n"
        );
        assert!(
            elapsed >= Duration::from_millis(250) && elapsed < Duration::from_secs(3),
            "answered at the request's deadline: {elapsed:?}"
        );
        assert!(!after_the_read.get(), "the read ends as a cancellation");
    }

    /// A connection idle between requests closes `keepAliveTimeout` plus node's
    /// one-second grace after its last answer, and the answer announces the
    /// timeout in whole seconds rounded down, as node's does (`200` ms is
    /// `timeout=0`). Switched off (`0`), nothing is announced.
    #[test]
    fn an_idle_connection_closes_after_the_keep_alive_timeout() {
        let (announced, idle_for, unannounced) = block_on(async move {
            let (server, port) = serve(echo_target());
            server.set_keep_alive_timeout(200);
            let (quiet, quiet_port) = serve(echo_target());
            quiet.set_keep_alive_timeout(0);
            let client = std::thread::spawn(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                stream
                    .write_all(b"GET / HTTP/1.1\r\nHost: a\r\n\r\n")
                    .expect("send");
                let announced = read_response(&mut stream);
                let answered = Instant::now();
                assert!(closed_within(&mut stream, Duration::from_secs(5)));
                let idle_for = answered.elapsed();
                let mut other = TcpStream::connect(("127.0.0.1", quiet_port)).expect("connect");
                other
                    .write_all(b"GET / HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n")
                    .expect("send");
                let _ = read_response(&mut other);
                let mut kept = TcpStream::connect(("127.0.0.1", quiet_port)).expect("connect");
                kept.write_all(b"GET / HTTP/1.1\r\nHost: a\r\n\r\n")
                    .expect("send");
                (announced, idle_for, read_response(&mut kept))
            });
            until(|| client.is_finished()).await;
            quiet.close(Rc::new(|| {}));
            finish(server, client).await
        });
        assert!(
            announced.contains("\r\nConnection: keep-alive\r\nKeep-Alive: timeout=0\r\n"),
            "{announced}"
        );
        assert!(
            idle_for >= Duration::from_millis(1100) && idle_for < Duration::from_secs(4),
            "closed at the timeout plus the grace: {idle_for:?}"
        );
        assert!(
            unannounced.contains("\r\nConnection: keep-alive\r\nContent-Length:"),
            "no `Keep-Alive` field when the timeout is off: {unannounced}"
        );
    }

    /// M164 on a kept connection: a body the handler left unread within the cap
    /// is discarded, and the NEXT request on the connection — sent in the same
    /// write, right behind the body — is read from where the body ended.
    #[test]
    fn an_unread_body_within_the_cap_is_discarded_and_the_next_request_served() {
        let answers = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                crate::executor::pin_future(async move {
                    // `std::http`'s `answer`, for a body within its cap.
                    assert!(!request.body_left_unread(1024));
                    response.end(&request.url());
                })
            });
            let (server, port) = serve(handler);
            let client = std::thread::spawn(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                stream
                    .write_all(
                        b"POST /upload HTTP/1.1\r\nHost: a\r\nContent-Length: 11\r\n\r\n\
                          hello worldGET /next HTTP/1.1\r\nHost: a\r\n\r\n",
                    )
                    .expect("send both");
                vec![read_response(&mut stream), read_response(&mut stream)]
            });
            finish(server, client).await
        });
        assert!(answers[0].ends_with("\r\n\r\n/upload"), "{answers:?}");
        assert!(answers[1].ends_with("\r\n\r\n/next"), "{answers:?}");
    }

    /// A body over the cap closes the connection even when the client asked to
    /// keep it: the `413` says `Connection: close`, and a request pipelined
    /// behind the refused body is never read.
    #[test]
    fn a_body_over_the_cap_closes_a_kept_connection() {
        let served = Rc::new(Cell::new(0));
        let count = Rc::clone(&served);
        let answered = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                let count = Rc::clone(&count);
                crate::executor::pin_future(async move {
                    count.set(count.get() + 1);
                    let _ = read_body(request.clone(), response.clone(), 4).await;
                    response.end("read");
                })
            });
            let (server, port) = serve(handler);
            let client = std::thread::spawn(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                stream
                    .write_all(
                        b"POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 11\r\n\r\n\
                          hello worldGET /next HTTP/1.1\r\nHost: a\r\n\r\n",
                    )
                    .expect("send both");
                let mut everything = String::new();
                let _ = stream.read_to_string(&mut everything);
                everything
            });
            finish(server, client).await
        });
        assert!(
            answered.starts_with(
                "HTTP/1.1 413 Payload Too Large\r\nContent-Type: text/plain\r\n\
                 Connection: close\r\nContent-Length: 39\r\n\r\n"
            ),
            "{answered}"
        );
        assert_eq!(answered.matches("HTTP/1.1").count(), 1, "{answered}");
        assert_eq!(served.get(), 1, "the pipelined request is never read");
    }

    /// A connection that upgrades leaves the request loop: a kept connection's
    /// SECOND request upgrades, the socket is the upgrade handler's, and none of
    /// the loop's deadlines reach it — short ones here, and the socket still
    /// echoes well past all of them.
    #[test]
    fn an_upgrade_on_a_kept_connection_leaves_the_request_loop() {
        let (first, echoed) = block_on(async move {
            let (server, port) = serve(echo_target());
            server.set_headers_timeout(200);
            server.set_request_timeout(200);
            server.set_keep_alive_timeout(100);
            server.on_upgrade(
                "upgrade",
                Rc::new(move |_request: Request, socket: Socket, _head: Bytes| {
                    socket.write_text("HTTP/1.1 101 Switching Protocols\r\n\r\n");
                    let echo = socket.clone();
                    socket.on_bytes(
                        "data",
                        Rc::new(move |bytes: Bytes| echo.write_bytes(&bytes)),
                    );
                }),
            );
            let client = std::thread::spawn(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                stream
                    .write_all(b"GET /plain HTTP/1.1\r\nHost: a\r\n\r\n")
                    .expect("send");
                let first = read_response(&mut stream);
                stream
                    .write_all(
                        b"GET /ws HTTP/1.1\r\nHost: a\r\nConnection: Upgrade\r\n\
                          Upgrade: websocket\r\n\r\n",
                    )
                    .expect("send the upgrade");
                let mut switched = [0u8; 36];
                stream.read_exact(&mut switched).expect("read the 101");
                std::thread::sleep(Duration::from_millis(1500));
                stream
                    .write_all(b"ping")
                    .expect("send on the upgraded socket");
                let mut echoed = [0u8; 4];
                stream.read_exact(&mut echoed).expect("read the echo");
                (first, String::from_utf8_lossy(&echoed).into_owned())
            });
            let answered = finish(server.clone(), client).await;
            server.terminate();
            answered
        });
        assert!(first.ends_with("GET /plain"), "{first}");
        assert_eq!(echoed, "ping");
    }

    /// A handler that outlives its exchange cannot reach into the next one: a
    /// read of its body started after the connection moved on ends as a
    /// cancellation, and the next request's handler still reads its own body.
    #[test]
    fn a_stale_request_cannot_read_the_next_requests_body() {
        let stale_read = Rc::new(Cell::new(false));
        let observed = Rc::clone(&stale_read);
        let answers = block_on(async move {
            let handler: Handler = Rc::new(move |request: Request, response: Response| {
                let observed = Rc::clone(&observed);
                crate::executor::pin_future(async move {
                    if request.url().to_string() == "/first" {
                        response.end("first");
                        crate::executor::sleep(200, None).await;
                        let _ = read_body(request.clone(), response.clone(), 1024).await;
                        observed.set(true);
                    } else {
                        let body = read_body(request.clone(), response.clone(), 1024).await;
                        response.end_bytes(&body);
                    }
                })
            });
            let (server, port) = serve(handler);
            let client = std::thread::spawn(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                stream
                    .write_all(b"GET /first HTTP/1.1\r\nHost: a\r\n\r\n")
                    .expect("send");
                let first = read_response(&mut stream);
                stream
                    .write_all(b"POST /second HTTP/1.1\r\nHost: a\r\nContent-Length: 6\r\n\r\n")
                    .expect("send the head");
                std::thread::sleep(Duration::from_millis(400));
                stream.write_all(b"second").expect("send the body");
                (first, read_response(&mut stream))
            });
            finish(server, client).await
        });
        assert!(answers.0.ends_with("first"), "{answers:?}");
        assert!(answers.1.ends_with("\r\n\r\nsecond"), "{answers:?}");
        assert!(!stale_read.get(), "the stale read ends as a cancellation");
    }

    /// `close()` closes a connection idle between requests at once, as node's
    /// does since 19 (`closeIdleConnections`) — a stop does not wait out the
    /// keep-alive timeout — and its callback fires once that connection is gone.
    #[test]
    fn closing_the_server_closes_an_idle_kept_connection_at_once() {
        let closed = Rc::new(Cell::new(false));
        let fired = Rc::clone(&closed);
        let (answer, ended_quickly) = block_on(async move {
            let (server, port) = serve(echo_target());
            let (answered, answered_signal) = std::sync::mpsc::channel::<()>();
            let client = std::thread::spawn(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                stream
                    .write_all(b"GET / HTTP/1.1\r\nHost: a\r\n\r\n")
                    .expect("send");
                let answer = read_response(&mut stream);
                answered.send(()).expect("report the answer");
                (answer, closed_within(&mut stream, Duration::from_secs(2)))
            });
            until(|| answered_signal.try_recv().is_ok()).await;
            server.close(Rc::new(move || fired.set(true)));
            until(|| client.is_finished()).await;
            let probe = server.clone();
            until(move || probe.0.connections.borrow().is_empty()).await;
            client.join().expect("the client thread")
        });
        assert!(answer.contains("Connection: keep-alive"), "{answer}");
        assert!(ended_quickly, "the idle connection is closed by the stop");
        assert!(closed.get(), "the close callback fires");
    }

    /// F163: `request.socket.remoteAddress` natively is the accepted
    /// connection's peer, formatted as node formats it for the same listener —
    /// `127.0.0.1` on an IPv4 bind, `::1` on an IPv6 one, and an IPv4 client of
    /// a dual-stack `::` bind as the mapped `::ffff:127.0.0.1` (Rust's
    /// `Ipv6Addr` display and node's agree on that spelling). Once the
    /// connection is gone it reads back `undefined`, as node's does.
    #[test]
    fn a_requests_socket_reports_the_peer_as_node_formats_it() {
        for (host, dial, expected) in [
            ("127.0.0.1", "127.0.0.1", "127.0.0.1"),
            ("::1", "::1", "::1"),
            ("::", "127.0.0.1", "::ffff:127.0.0.1"),
        ] {
            // F164: Windows sockets are IPv6-only by default, so a `::` bind is
            // not dual-stack there until the runtime clears `IPV6_V6ONLY` as
            // node does; an IPv4 client is refused, and this case waits on that.
            if cfg!(windows) && host == "::" {
                continue;
            }
            let seen = Rc::new(RefCell::new(Vec::<Request>::new()));
            let kept = Rc::clone(&seen);
            let (answered, peer, after) = block_on(async move {
                let handler: Handler = Rc::new(move |request: Request, response: Response| {
                    let kept = Rc::clone(&kept);
                    crate::executor::pin_future(async move {
                        let peer = match request.socket().remote_address_raw() {
                            JsonValue::Text(text) => text.to_string(),
                            other => format!("{other:?}"),
                        };
                        kept.borrow_mut().push(request);
                        response.end(&peer);
                    })
                });
                let server = create_server(handler);
                server.listen_on(0, host, Rc::new(|| {}));
                let port = server.port() as u16;
                let client = std::thread::spawn(move || {
                    let mut stream = TcpStream::connect((dial, port)).expect("connect");
                    stream
                        .write_all(b"GET / HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n")
                        .expect("send");
                    let mut response = String::new();
                    stream.read_to_string(&mut response).expect("read");
                    response
                });
                until(|| client.is_finished()).await;
                let probe = server.clone();
                until(move || probe.0.connections.borrow().is_empty()).await;
                server.close(Rc::new(|| {}));
                let request = seen.borrow()[0].clone();
                let after = request.socket().remote_address_raw();
                let answered = client.join().expect("the client thread");
                let (_, _, body) = split(&answered);
                (answered, body, after)
            });
            assert!(answered.starts_with("HTTP/1.1 200"), "{host}: {answered}");
            assert_eq!(peer, expected, "{host}: the peer as node spells it");
            assert!(
                matches!(after, JsonValue::Undefined),
                "{host}: a closed connection's peer is undefined: {after:?}"
            );
        }
    }
}
