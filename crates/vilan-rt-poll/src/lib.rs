//! `vilan-rt-poll` — the readiness wait behind a native program's event loop
//! (tracker F160).
//!
//! # What it does
//!
//! [`install`] hands `vilan-rt`'s loop ([`vilan_rt::executor::install_readiness`])
//! a wait that blocks until one of the sockets the loop's sources watch is
//! ready, or the nearest deadline passes: `poll(2)` on unix, `WSAPoll` on
//! Windows. Without it the loop polls its sources on a 1 ms tick — an idle
//! server woke a thousand times a second and every request waited up to a
//! millisecond for the next look; with it an idle server sleeps in the kernel
//! and a request is answered as soon as its bytes arrive.
//!
//! The wait is LEVEL-triggered and stateless: each call is handed every socket
//! the turn watches and registers nothing, so it cannot fall out of step with
//! the sources, and the loop's turn — poll every source, one macrotask, drain
//! the microtasks — is exactly what it was. Which sockets, which deadline, and
//! the loopback "bell" another thread rings to end the wait are all
//! `vilan-rt`'s; this crate is the one call `vilan-rt` cannot make without
//! `unsafe`.
//!
//! # Why a crate of its own
//!
//! `vilan-rt` takes no crates.io dependencies and forbids `unsafe` (the
//! manifest says why), and Rust's std has no way to wait on several sockets
//! at once. Called once, from the emitted `main`, before the program's body
//! runs — only by a program that does I/O the loop waits on.

#![deny(unsafe_op_in_unsafe_fn)]

use std::io;
use std::time::Duration;

use vilan_rt::executor::Watch;

/// Installs [`wait`] as the loop's readiness wait. Idempotent: the first
/// install wins.
pub fn install() {
    vilan_rt::executor::install_readiness(wait);
}

/// The wait's timeout in whole milliseconds, ROUNDED UP — a deadline 0.3 ms
/// away waits 1 ms rather than 0, which would be a spin until it passed — and
/// `-1` (no limit) for `None`. Both `poll` and `WSAPoll` take this shape.
fn timeout_millis(timeout: Option<Duration>) -> i32 {
    let Some(timeout) = timeout else {
        return -1;
    };
    let whole = timeout.as_millis();
    let rounded = if timeout > Duration::from_millis(whole as u64) {
        whole + 1
    } else {
        whole
    };
    i32::try_from(rounded).unwrap_or(i32::MAX)
}

/// Blocks until one of `watches` is ready for what it is watched for (or
/// reports an error or a hang-up — `poll` always reports those), or `timeout`
/// passes, and marks each [`Watch::ready`] that reported anything. An
/// interrupted wait returns early and `Ok` with every watch marked: nothing is
/// known, so the loop's next poll tries them all.
#[cfg(unix)]
pub fn wait(watches: &mut [Watch], timeout: Option<Duration>) -> io::Result<()> {
    let mut descriptors: Vec<libc::pollfd> = watches
        .iter()
        .map(|watch| libc::pollfd {
            fd: watch.socket,
            events: (if watch.readable { libc::POLLIN } else { 0 })
                | (if watch.writable { libc::POLLOUT } else { 0 }),
            revents: 0,
        })
        .collect();
    let count = libc::nfds_t::try_from(descriptors.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "too many sockets to watch"))?;
    // SAFETY: `descriptors` is a live, exclusively borrowed array of `count`
    // initialized `pollfd`s for the duration of the call, which is all `poll`
    // reads and writes (it sets each `revents`). The descriptors are sockets
    // the loop's sources own and keep open across the call — the loop is
    // single-threaded and blocked here — and an fd that is not open would be
    // answered `POLLNVAL`, not undefined behaviour.
    let answered = unsafe { libc::poll(descriptors.as_mut_ptr(), count, timeout_millis(timeout)) };
    if answered < 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            for watch in watches.iter_mut() {
                watch.ready = true;
            }
            return Ok(());
        }
        return Err(error);
    }
    for (watch, descriptor) in watches.iter_mut().zip(&descriptors) {
        watch.ready = descriptor.revents != 0;
    }
    Ok(())
}

/// Blocks until one of `watches` is ready for what it is watched for (or
/// reports an error or a hang-up — `WSAPoll` always reports those), or
/// `timeout` passes, and marks each [`Watch::ready`] that reported anything.
#[cfg(windows)]
pub fn wait(watches: &mut [Watch], timeout: Option<Duration>) -> io::Result<()> {
    use windows_sys::Win32::Networking::WinSock::{
        POLLRDNORM, POLLWRNORM, SOCKET, SOCKET_ERROR, WSAGetLastError, WSAPOLLFD, WSAPoll,
    };
    let mut descriptors: Vec<WSAPOLLFD> = watches
        .iter()
        .map(|watch| WSAPOLLFD {
            // A `RawSocket` is the `SOCKET`'s value, widened to `u64`.
            fd: watch.socket as SOCKET,
            // `WSAPoll` refuses the flags it does not support (`POLLPRI`), so
            // the two NORMAL-data flags are named rather than `POLLIN`'s pair.
            events: (if watch.readable { POLLRDNORM } else { 0 })
                | (if watch.writable { POLLWRNORM } else { 0 }),
            revents: 0,
        })
        .collect();
    let count = u32::try_from(descriptors.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "too many sockets to watch"))?;
    // SAFETY: `descriptors` is a live, exclusively borrowed array of `count`
    // initialized `WSAPOLLFD`s for the duration of the call, which is all
    // `WSAPoll` reads and writes (it sets each `revents`). The sockets are
    // ones the loop's sources own and keep open across the call, and Winsock
    // is initialized: every one of them was opened through `std::net`, which
    // starts it. The loop always watches its bell, so `count` is never 0.
    let answered = unsafe { WSAPoll(descriptors.as_mut_ptr(), count, timeout_millis(timeout)) };
    if answered == SOCKET_ERROR {
        // SAFETY: reads the calling thread's last Winsock error; no arguments.
        let code = unsafe { WSAGetLastError() };
        return Err(io::Error::from_raw_os_error(code));
    }
    for (watch, descriptor) in watches.iter_mut().zip(&descriptors) {
        watch.ready = descriptor.revents != 0;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timeout_rounds_up_to_whole_milliseconds_and_none_is_no_limit() {
        assert_eq!(timeout_millis(None), -1);
        assert_eq!(timeout_millis(Some(Duration::ZERO)), 0);
        assert_eq!(timeout_millis(Some(Duration::from_micros(300))), 1);
        assert_eq!(timeout_millis(Some(Duration::from_millis(5))), 5);
        assert_eq!(timeout_millis(Some(Duration::from_micros(5_001))), 6);
        assert_eq!(
            timeout_millis(Some(Duration::from_secs(u64::MAX))),
            i32::MAX
        );
    }
}
