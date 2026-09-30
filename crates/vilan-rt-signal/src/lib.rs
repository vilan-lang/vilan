//! `vilan-rt-signal` — a native server's graceful stop on a termination
//! signal (tracker F45; Order 43's R-i, RULED: build).
//!
//! # What it does
//!
//! [`install`] asks the operating system to deliver SIGTERM, SIGINT and SIGHUP
//! (on Windows: Ctrl-C and the console's close, logoff and shutdown events) to
//! [`vilan_rt::http::request_termination`] instead of killing the process
//! where it stands. The first request is the GRACEFUL stop: every server stops
//! accepting, closes what is waiting on a request or holds an upgraded socket,
//! lets a request already being answered finish, and the event loop runs out
//! of work — so `main` returns, and whatever runs at process end (the leak
//! census among it) runs. A SECOND request ends the process at once, for the
//! server whose open response never ends.
//!
//! # Why a crate of its own
//!
//! `vilan-rt` takes no crates.io dependencies and forbids `unsafe`, and a
//! signal handler can be installed with neither (the manifest says why). The
//! stop itself — what a request DOES — is `vilan-rt`'s and dependency-free;
//! this crate is only the wire from the OS to it.
//!
//! # A divergence from node, on purpose
//!
//! Node, with no `process.on("SIGTERM")` handler, dies of the signal. The
//! native server drains first — the ruling's whole point: a server program
//! reaches its process end, where a census can read it.

#![forbid(unsafe_code)]

/// Routes the termination signals to [`vilan_rt::http::request_termination`].
///
/// Best effort, and idempotent in effect: a handler the OS refuses (or a
/// second install, which `ctrlc` refuses) leaves the default disposition in
/// place, which is node's own behaviour — the process dies of the signal.
/// Called once, from the emitted `main`, before the program's body runs.
pub fn install() {
    let _ = ctrlc::set_handler(vilan_rt::http::request_termination);
}
