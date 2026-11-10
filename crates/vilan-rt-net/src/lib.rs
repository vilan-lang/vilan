//! `vilan-rt-net` — a native server's IPv6 listening socket, bound the way
//! node binds one (tracker F164).
//!
//! # What it does
//!
//! [`bind_dual_stack`] opens an IPv6 TCP socket, turns `IPV6_V6ONLY` OFF, and
//! only then binds and listens. That is libuv's `uv_tcp_bind` without
//! `UV_TCP_IPV6ONLY`, which is every IPv6 bind node's `net.Server` makes: so
//! `listen(port)` with no host (node's `::`) and an explicit `::` both take
//! IPv4 clients too, which then read back mapped (`::ffff:127.0.0.1`).
//!
//! Rust's std cannot say this: `TcpListener::bind` creates, binds and listens
//! in one call, and `IPV6_V6ONLY` can only be set before the bind. On Linux and
//! macOS the option is off by default (`net.ipv6.bindv6only = 0`), so std's
//! bind happened to agree with node there; on Windows it is ON by default, and
//! a `::` listener refused every IPv4 client. Clearing it explicitly makes the
//! answer the same on every OS, and on a Linux machine whose administrator
//! turned `bindv6only` on, too — node clears it there as well.
//!
//! The socket keeps std's other choices: `SO_REUSEADDR` on unix (libuv and
//! std both set it; on Windows neither does, since there it means sharing a
//! bound port), close-on-exec / no handle inheritance, and node's listen
//! backlog of 511.
//!
//! # Why a crate of its own
//!
//! `vilan-rt` takes no crates.io dependencies and forbids `unsafe`; the socket
//! option is FFI. This crate depends on nothing of vilan's and answers a std
//! [`TcpListener`], so `vilan-rt` reaches it only through the seam
//! `vilan_rt::http::set_ipv6_binder`, which `vilan-rt-signal::install`
//! fills for every program that starts a server.

#![deny(unsafe_op_in_unsafe_fn)]

use std::io;
use std::net::{SocketAddrV6, TcpListener};

/// node's listen backlog (`net.Server`'s default, 511).
const BACKLOG: i32 = 511;

/// Binds and listens on `address` with `IPV6_V6ONLY` cleared — a dual-stack
/// listener when the address is `::`, exactly as node's.
///
/// The error is the OS's own, so a caller can tell an address in use
/// ([`io::ErrorKind::AddrInUse`]) from a machine without IPv6 (node falls
/// back to `0.0.0.0` for the second and not the first).
pub fn bind_dual_stack(address: SocketAddrV6) -> io::Result<TcpListener> {
    platform::bind_dual_stack(address)
}

#[cfg(unix)]
mod platform {
    use super::BACKLOG;
    use std::io;
    use std::net::{SocketAddrV6, TcpListener};
    use std::os::fd::{FromRawFd, OwnedFd};

    pub fn bind_dual_stack(address: SocketAddrV6) -> io::Result<TcpListener> {
        let socket = new_socket()?;
        let descriptor = std::os::fd::AsRawFd::as_raw_fd(&socket);
        set_option(descriptor, libc::IPPROTO_IPV6, libc::IPV6_V6ONLY, 0)?;
        set_option(descriptor, libc::SOL_SOCKET, libc::SO_REUSEADDR, 1)?;
        let raw = socket_address(address);
        // SAFETY: `raw` is a fully initialised `sockaddr_in6` that outlives the
        // call, and the length passed is its size.
        let bound = unsafe {
            libc::bind(
                descriptor,
                (&raw as *const libc::sockaddr_in6).cast::<libc::sockaddr>(),
                size_of::<libc::sockaddr_in6>() as libc::socklen_t,
            )
        };
        if bound != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `descriptor` is the open, bound socket `socket` owns.
        if unsafe { libc::listen(descriptor, BACKLOG) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(TcpListener::from(socket))
    }

    /// A new IPv6 stream socket, close-on-exec, owned (closed on every early
    /// return above).
    fn new_socket() -> io::Result<OwnedFd> {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let kind = libc::SOCK_STREAM | libc::SOCK_CLOEXEC;
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        let kind = libc::SOCK_STREAM;
        // SAFETY: a plain syscall with constant arguments.
        let descriptor = unsafe { libc::socket(libc::AF_INET6, kind, 0) };
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `socket` just returned this descriptor and nothing else owns it.
        let socket = unsafe { OwnedFd::from_raw_fd(descriptor) };
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        {
            // SAFETY: `descriptor` is open and owned by `socket`.
            if unsafe { libc::fcntl(descriptor, libc::F_SETFD, libc::FD_CLOEXEC) } != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(socket)
    }

    fn set_option(
        descriptor: libc::c_int,
        level: libc::c_int,
        name: libc::c_int,
        value: libc::c_int,
    ) -> io::Result<()> {
        // SAFETY: `value` is a live `c_int` and the length passed is its size.
        let result = unsafe {
            libc::setsockopt(
                descriptor,
                level,
                name,
                (&value as *const libc::c_int).cast::<libc::c_void>(),
                size_of::<libc::c_int>() as libc::socklen_t,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn socket_address(address: SocketAddrV6) -> libc::sockaddr_in6 {
        // SAFETY: `sockaddr_in6` is plain old data, valid all-zero.
        let mut raw: libc::sockaddr_in6 = unsafe { std::mem::zeroed() };
        #[cfg(any(
            target_vendor = "apple",
            target_os = "freebsd",
            target_os = "dragonfly",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        {
            raw.sin6_len = size_of::<libc::sockaddr_in6>() as u8;
        }
        raw.sin6_family = libc::AF_INET6 as libc::sa_family_t;
        raw.sin6_port = address.port().to_be();
        raw.sin6_flowinfo = address.flowinfo();
        raw.sin6_addr = libc::in6_addr {
            s6_addr: address.ip().octets(),
        };
        raw.sin6_scope_id = address.scope_id();
        raw
    }
}

#[cfg(windows)]
mod platform {
    use super::BACKLOG;
    use std::io;
    use std::net::{SocketAddrV6, TcpListener};
    use std::os::windows::io::{FromRawSocket, OwnedSocket, RawSocket};
    use std::sync::OnceLock;
    use windows_sys::Win32::Networking::WinSock as winsock;

    pub fn bind_dual_stack(address: SocketAddrV6) -> io::Result<TcpListener> {
        startup()?;
        // SAFETY: a plain call with constant arguments and no protocol info.
        let handle = unsafe {
            winsock::WSASocketW(
                i32::from(winsock::AF_INET6),
                winsock::SOCK_STREAM,
                winsock::IPPROTO_TCP,
                std::ptr::null(),
                0,
                winsock::WSA_FLAG_OVERLAPPED | winsock::WSA_FLAG_NO_HANDLE_INHERIT,
            )
        };
        if handle == winsock::INVALID_SOCKET {
            return Err(last_error());
        }
        // SAFETY: `WSASocketW` just returned this socket and nothing else owns
        // it; the `OwnedSocket` closes it on every early return below.
        let socket = unsafe { OwnedSocket::from_raw_socket(handle as RawSocket) };
        let off: i32 = 0;
        // SAFETY: `off` is a live `i32` (the option's `DWORD`) and the length
        // passed is its size.
        let set = unsafe {
            winsock::setsockopt(
                handle,
                winsock::IPPROTO_IPV6,
                winsock::IPV6_V6ONLY,
                (&off as *const i32).cast::<u8>(),
                size_of::<i32>() as i32,
            )
        };
        if set != 0 {
            return Err(last_error());
        }
        let raw = winsock::SOCKADDR_IN6 {
            sin6_family: winsock::AF_INET6,
            sin6_port: address.port().to_be(),
            sin6_flowinfo: address.flowinfo(),
            sin6_addr: winsock::IN6_ADDR {
                u: winsock::IN6_ADDR_0 {
                    Byte: address.ip().octets(),
                },
            },
            Anonymous: winsock::SOCKADDR_IN6_0 {
                sin6_scope_id: address.scope_id(),
            },
        };
        // SAFETY: `raw` is a fully initialised `SOCKADDR_IN6` that outlives the
        // call, and the length passed is its size.
        let bound = unsafe {
            winsock::bind(
                handle,
                (&raw as *const winsock::SOCKADDR_IN6).cast::<winsock::SOCKADDR>(),
                size_of::<winsock::SOCKADDR_IN6>() as i32,
            )
        };
        if bound != 0 {
            return Err(last_error());
        }
        // SAFETY: `handle` is the open, bound socket `socket` owns.
        if unsafe { winsock::listen(handle, BACKLOG) } != 0 {
            return Err(last_error());
        }
        Ok(TcpListener::from(socket))
    }

    /// Winsock must be started before the first socket call. std starts it
    /// lazily on ITS first network call, which a server bound here may never
    /// have made, so this starts it too — `WSAStartup` is reference-counted,
    /// and std's own start (and its cleanup at exit) is unaffected.
    fn startup() -> io::Result<()> {
        static STARTED: OnceLock<i32> = OnceLock::new();
        let result = *STARTED.get_or_init(|| {
            let mut data = winsock::WSADATA::default();
            // SAFETY: `data` is a live, writable `WSADATA`.
            unsafe { winsock::WSAStartup(0x0202, &mut data) }
        });
        if result != 0 {
            return Err(io::Error::from_raw_os_error(result));
        }
        Ok(())
    }

    fn last_error() -> io::Error {
        // SAFETY: a plain call reading this thread's last Winsock error.
        io::Error::from_raw_os_error(unsafe { winsock::WSAGetLastError() })
    }
}

#[cfg(test)]
mod tests {
    use super::bind_dual_stack;
    use std::io::{Read, Write};
    use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, TcpStream};

    /// Accepts one connection from `dial` and answers what the listener saw
    /// as its peer, spelled as node spells `remoteAddress`. The connect comes
    /// FIRST, on this thread: the kernel completes it into the listen backlog
    /// before any accept, so a refused dial fails here rather than leaving an
    /// accept waiting forever.
    fn peer_of(listener: &std::net::TcpListener, dial: &str) -> String {
        let port = listener.local_addr().expect("bound").port();
        let mut stream = TcpStream::connect((dial, port)).expect("connect");
        stream.write_all(b"x").expect("send");
        let (mut accepted, peer) = listener.accept().expect("accept");
        let mut byte = [0u8; 1];
        accepted.read_exact(&mut byte).expect("read");
        match peer {
            SocketAddr::V4(peer) => peer.ip().to_string(),
            SocketAddr::V6(peer) => peer.ip().to_string(),
        }
    }

    /// F164: `::` is dual-stack — an IPv4 client is accepted and reads back
    /// mapped, an IPv6 one as itself — because `IPV6_V6ONLY` is cleared,
    /// which on Windows (where it defaults on) is the difference.
    #[test]
    fn the_unspecified_address_takes_ipv4_and_ipv6_clients() {
        let listener =
            bind_dual_stack(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0)).expect("bind ::");
        assert!(listener.local_addr().expect("bound").is_ipv6());
        assert_eq!(peer_of(&listener, "127.0.0.1"), "::ffff:127.0.0.1");
        assert_eq!(peer_of(&listener, "::1"), "::1");
    }

    /// `::1` is IPv6 loopback alone, cleared option or not: node's `::1`
    /// listener refuses an IPv4 client, and so does this one.
    #[test]
    fn the_ipv6_loopback_takes_ipv6_clients_only() {
        let listener =
            bind_dual_stack(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0)).expect("bind ::1");
        assert_eq!(peer_of(&listener, "::1"), "::1");
        let port = listener.local_addr().expect("bound").port();
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
    }

    /// A port already held is the OS's `AddrInUse`, which a caller needs to
    /// tell apart from "no IPv6 here" (node falls back to IPv4 for the second
    /// only).
    #[test]
    fn a_port_in_use_is_reported_as_in_use() {
        let first =
            bind_dual_stack(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0)).expect("bind ::");
        let port = first.local_addr().expect("bound").port();
        let second = bind_dual_stack(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0));
        assert_eq!(
            second.expect_err("the port is held").kind(),
            std::io::ErrorKind::AddrInUse
        );
    }
}
