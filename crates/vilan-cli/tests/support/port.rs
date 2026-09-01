//! A server that CHOOSES its own port, and the harness end that learns it.
//!
//! The e2e suites here used to bind `127.0.0.1:0`, read the port, DROP the
//! listener, and bake the number into a server they then compiled and spawned —
//! a bind-release-rebind window a whole `vilan build` wide. Under ~10 parallel
//! suites that window is not theoretical: N40 caught `serve_build`'s
//! `a_binary_artifact_reaches_the_wire_as_the_build_wrote_it` failing with "the
//! server should bind 45673" after 114 s while passing 10/10 alone, and the same
//! shape had already struck three times in one day (`http_port.rs`'s header).
//!
//! There is no window to shrink here, because there is no release: the SERVER
//! binds — `.port(0)`, so the OS picks — and reports the number back through
//! `on_start`, which `std::http` has carried since E19 and `http_port.rs` pins
//! end to end. The port cannot be taken between the bind and the report, since
//! the bind still holds it.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// The `on_start` body a server under test carries so the harness can read the
/// port the OS gave it.
///
/// One constant serves both ends — the vilan expression written into the server
/// source and, through [`ANNOUNCEMENT`], the prefix the harness scans stdout for
/// — so the two cannot drift apart.
pub const ANNOUNCE_PORT: &str = r#"print(i"vilan-test-port={server.port()}")"#;

/// The prefix [`ANNOUNCE_PORT`] prints, and the only thing the harness parses.
const ANNOUNCEMENT: &str = "vilan-test-port=";

/// A spawned server whose port is the one it actually bound.
///
/// Killed on drop, so a failed assertion cannot leak a listener into the rest of
/// the suite.
pub struct Server {
    child: Child,
    lines: Receiver<String>,
    /// Every stdout line read so far, announcement included — a server's boot
    /// output is evidence some pins assert on, and the reader thread has
    /// already consumed it off the pipe by the time they ask.
    seen: Vec<String>,
    port: u16,
}

impl Server {
    /// Spawn `command` and wait for it to announce the port it bound.
    ///
    /// `command` must be a built vilan server whose `on_start` carries
    /// [`ANNOUNCE_PORT`]; stdout is claimed here (piped and drained by a reader
    /// thread), so a caller that wants the boot output asks [`Server::stdout`]
    /// for it rather than reading the pipe.
    ///
    /// The wait is a LIVENESS bound — `run_liveness()`, the same budget the rest
    /// of this harness spends on "a spawned program must eventually get going" —
    /// not a claim about how fast a server boots. A green spawn returns the
    /// moment the line arrives.
    pub fn spawn(command: &mut Command) -> Server {
        let mut child = command
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn the server");
        let stdout = child.stdout.take().expect("the server's stdout");
        let (sender, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });

        let mut server = Server {
            child,
            lines,
            seen: Vec::new(),
            port: 0,
        };
        server.port = server.await_announcement(super::run_liveness());
        server
    }

    /// The port the server bound — reported by the server itself, never guessed.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Everything the server has written to stdout, including whatever arrived
    /// after the announcement.
    ///
    /// Drains without blocking, so it reports whatever has landed by now; call
    /// it after [`Server::stop`] to be sure the process is done writing.
    pub fn stdout(&mut self) -> String {
        while let Ok(line) = self.lines.try_recv() {
            self.seen.push(line);
        }
        self.seen.join("\n")
    }

    /// Kill the server and reap it. Idempotent — a second call is a no-op on an
    /// already-dead child.
    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn await_announcement(&mut self, timeout: Duration) -> u16 {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                panic!(
                    "the server never announced its port within {timeout:?}:\n{}",
                    self.seen.join("\n")
                );
            }
            match self.lines.recv_timeout(remaining) {
                Ok(line) => {
                    let announced = line
                        .split_whitespace()
                        .find_map(|field| field.strip_prefix(ANNOUNCEMENT))
                        .map(str::to_string);
                    self.seen.push(line);
                    if let Some(announced) = announced {
                        let port: u16 = announced
                            .parse()
                            .unwrap_or_else(|_| panic!("`{ANNOUNCEMENT}` carried `{announced}`"));
                        assert_ne!(
                            port, 0,
                            "the server reported the port it asked for, not one it bound"
                        );
                        return port;
                    }
                }
                Err(_) => panic!(
                    "the server's stdout ended before it announced a port:\n{}",
                    self.seen.join("\n")
                ),
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}
