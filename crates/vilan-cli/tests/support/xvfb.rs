//! A program in a virtual X display, for the suites that run a native window
//! end to end under `xvfb-run -a` (`gfx_window.rs`, `ui_screen.rs`,
//! `ui_interaction.rs`): its stdout arriving line by line, the display it
//! runs on (for the XTEST injector), and — whatever the test body does — no
//! process of the run left behind (N198).
//!
//! # The process group
//!
//! A run is `xvfb-run` (a shell script), the `Xvfb` server it starts, and the
//! program it execs: three processes, of which the test holds only the first.
//! Killing that one leaves the other two — a `--smoke` list was found alive 73
//! minutes after a lane's run, holding a display and a software GPU device. So
//! the run is started in a PROCESS GROUP of its own, and [`Running`]'s `Drop`
//! ends the whole group: SIGTERM first (`xvfb-run` and `Xvfb` clean up their
//! lock and authority files on it), then, for whatever is still alive after a
//! grace period, SIGKILL; and it does not return until no live process of the
//! group remains. A test body that panics, a wait that times out and a run
//! that exited on its own all go through that one door.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long one run may take before it is a hang: a few seconds warm (one of
/// them a program's own idle second); the bound is for a loaded runner.
pub const RUN_LIMIT: Duration = Duration::from_secs(120);

/// How long the group has to end on SIGTERM before it is killed.
const TERM_GRACE: Duration = Duration::from_secs(5);

/// How long a KILLED group may take to be gone before the harness says so.
const KILL_LIMIT: Duration = Duration::from_secs(30);

/// A program in a virtual display, its stdout arriving line by line. The
/// first line is the wrapper's: the display and its authority file, which the
/// injector connects with.
pub struct Running {
    child: Child,
    lines: mpsc::Receiver<String>,
    /// Every stdout line read so far, in order. `xvfb-run` may merge the
    /// program's stderr into it (ubuntu-latest's does), so a message is
    /// counted across both streams.
    pub seen: Vec<String>,
    display: String,
    authority: String,
    /// The window's title, by which the injector finds it.
    title: String,
}

impl Running {
    /// Starts `binary` with `arguments` under `xvfb-run -a`, in a process
    /// group of its own, and waits for the wrapper to name its display.
    pub fn start(binary: &Path, arguments: &[&str], title: &str) -> Running {
        Running::start_in(binary, arguments, title, &[])
    }

    /// [`Running::start`] with `environment` added to the program's (a
    /// private session bus, say: `support::atspi`).
    pub fn start_in(
        binary: &Path,
        arguments: &[&str],
        title: &str,
        environment: &[(String, String)],
    ) -> Running {
        let mut command = Command::new("xvfb-run");
        command.envs(environment.iter().map(|(name, value)| (name, value)));
        command
            .args(["-a", "-s", "-screen 0 1280x1024x24", "sh", "-c"])
            .arg("printf 'display %s %s\\n' \"$DISPLAY\" \"$XAUTHORITY\"; exec \"$0\" \"$@\"")
            .arg(binary)
            .args(arguments)
            // X11 under xvfb whatever session the suite runs in, at 100%.
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("WAYLAND_SOCKET")
            .env("WINIT_X11_SCALE_FACTOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // A group of its own, led by `xvfb-run`: `Xvfb` and the program
            // join it, and `Drop` ends all three.
            command.process_group(0);
        }
        let mut child = command.spawn().expect("start the program under xvfb-run");
        let stdout = child.stdout.take().expect("piped stdout");
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let mut running = Running {
            child,
            lines,
            seen: Vec::new(),
            display: String::new(),
            authority: String::new(),
            title: title.to_string(),
        };
        // From here a failure drops `running`, which ends the group.
        let first = running
            .lines
            .recv_timeout(RUN_LIMIT)
            .expect("the wrapper names the display");
        let mut words = first.split_whitespace();
        assert_eq!(words.next(), Some("display"), "{first}");
        running.display = words.next().expect("a display").to_string();
        running.authority = words.next().unwrap_or_default().to_string();
        running
    }

    /// The process group of the run: `xvfb-run`'s pid, which leads it.
    pub fn group(&self) -> u32 {
        self.child.id()
    }

    /// Waits for a line `wanted` accepts, recording every line on the way;
    /// answers it. A run that never prints one is a failure naming `what`,
    /// with everything it printed.
    pub fn wait_until(
        &mut self,
        what: &str,
        started: Instant,
        wanted: impl Fn(&str) -> bool,
    ) -> String {
        loop {
            let left = RUN_LIMIT.saturating_sub(started.elapsed());
            match self.lines.recv_timeout(left) {
                Ok(line) => {
                    let found = wanted(&line);
                    self.seen.push(line.clone());
                    if found {
                        return line;
                    }
                }
                Err(_) => {
                    end_group(&mut self.child);
                    let stderr = self.stderr();
                    panic!(
                        "the program never printed {what}; it printed:\n{}\n--- stderr\n{stderr}",
                        self.seen.join("\n")
                    );
                }
            }
        }
    }

    /// Waits for a line containing `wanted`; answers it.
    pub fn wait_for(&mut self, wanted: &str, started: Instant) -> String {
        self.wait_until(&format!("{wanted:?}"), started, |line| {
            line.contains(wanted)
        })
    }

    /// Everything the program wrote to stderr, once it has closed it.
    pub fn stderr(&mut self) -> String {
        let mut text = String::new();
        if let Some(mut stderr) = self.child.stderr.take() {
            let _ = std::io::Read::read_to_string(&mut stderr, &mut text);
        }
        text
    }

    /// Moves, presses, releases, turns the wheel, types or closes, through the
    /// X server, and returns once the server has handled it.
    pub fn inject(&self, injector: &Path, commands: &[&str]) {
        let status = Command::new(injector)
            .args(["--title", &self.title])
            .args(commands)
            .env("DISPLAY", &self.display)
            .env("XAUTHORITY", &self.authority)
            .status()
            .expect("run the injector");
        assert!(status.success(), "injecting {commands:?} failed: {status}");
    }

    /// Starts the injector with `commands` against this run's display and
    /// does NOT wait for it: for a command that serves until the program acts
    /// (`clipboard own`). Its output is piped for the caller to read, and the
    /// caller waits for it.
    pub fn start_injector(&self, injector: &Path, commands: &[&str]) -> std::process::Child {
        Command::new(injector)
            .args(["--title", &self.title])
            .args(commands)
            .env("DISPLAY", &self.display)
            .env("XAUTHORITY", &self.authority)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("run the injector")
    }

    /// Runs the injector with `commands` against this run's display, as
    /// [`Running::inject`] does, and answers what it printed.
    pub fn inject_output(&self, injector: &Path, commands: &[&str]) -> String {
        let output = Command::new(injector)
            .args(["--title", &self.title])
            .args(commands)
            .env("DISPLAY", &self.display)
            .env("XAUTHORITY", &self.authority)
            .output()
            .expect("run the injector");
        assert!(
            output.status.success(),
            "injecting {commands:?} failed: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    /// Waits for the program to exit: its code, every stdout line, stderr.
    pub fn exit(mut self, started: Instant) -> (Option<i32>, Vec<String>, String) {
        loop {
            if let Some(status) = self.child.try_wait().expect("wait on the program") {
                // The reader thread ends at the pipe's end, after the last
                // line; collect them all.
                while let Ok(line) = self.lines.recv_timeout(Duration::from_secs(5)) {
                    self.seen.push(line);
                }
                let stderr = self.stderr();
                return (status.code(), std::mem::take(&mut self.seen), stderr);
            }
            if started.elapsed() > RUN_LIMIT {
                end_group(&mut self.child);
                panic!(
                    "the program did not exit; it printed:\n{}",
                    self.seen.join("\n")
                );
            }
            // A bounded poll of a process that has ALREADY been asked to end
            // (the ordering is the output's): the interval only sets how soon
            // the exit is noticed.
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Running {
    /// Ends the run's whole process group, and returns only when none of it
    /// is alive (see the module documentation).
    fn drop(&mut self) {
        end_group(&mut self.child);
    }
}

/// SIGTERM to `leader`'s group, SIGKILL to whatever outlives the grace
/// period, and a wait until no live process of the group remains; `leader`
/// itself is reaped. A group that is already gone costs one `/proc` scan.
#[cfg(unix)]
fn end_group(leader: &mut Child) {
    let group = leader.id();
    for (signal, limit) in [("-TERM", TERM_GRACE), ("-KILL", KILL_LIMIT)] {
        if live_in_group(group).is_empty() {
            break;
        }
        // `kill -SIG -- -GROUP` signals every process of the group.
        let _ = Command::new("kill")
            .args([signal, "--", &format!("-{group}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        // Reap the leader as soon as it ends, or it stays a zombie of ours
        // and its group never reads as empty.
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline {
            let _ = leader.try_wait();
            if live_in_group(group).is_empty() {
                break;
            }
            // A bounded wait for processes ALREADY signalled to end: the
            // interval only sets how soon their end is noticed.
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let _ = leader.try_wait();
    let left = live_in_group(group);
    assert!(
        left.is_empty() || std::thread::panicking(),
        "the run's processes {left:?} outlived SIGKILL"
    );
}

#[cfg(not(unix))]
fn end_group(leader: &mut Child) {
    // No `xvfb-run` here: the suites that start one skip off Linux.
    let _ = leader.kill();
    let _ = leader.wait();
}

/// The live (not zombie) processes of process group `group`, read from
/// `/proc`: what `Drop` waits on, and what the N198 pin reads.
#[cfg(unix)]
pub fn live_in_group(group: u32) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            // `/proc/PID/stat`: `pid (comm) state ppid pgrp ...`; the command
            // name may hold spaces and parentheses, so fields are read after
            // its LAST `)`.
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                return false;
            };
            let Some((_, rest)) = stat.rsplit_once(')') else {
                return false;
            };
            let fields: Vec<&str> = rest.split_whitespace().collect();
            fields.first().is_some_and(|state| *state != "Z")
                && fields.get(2).and_then(|pgrp| pgrp.parse::<u32>().ok()) == Some(group)
        })
        .collect()
}

#[cfg(not(unix))]
pub fn live_in_group(_group: u32) -> Vec<u32> {
    Vec::new()
}
