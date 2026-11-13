//! A private accessibility bus for the suites that check what a native
//! window publishes to assistive technology (native-ui.md P5): a session
//! D-Bus of the test's own, AT-SPI's bus launcher on it with the
//! accessibility switch on, and the tree read and acted on through `gdbus`
//! (the box has no `pyatspi`), as a screen reader would.
//!
//! # Private, and gone afterwards
//!
//! Nothing touches the user's own session: the bus is the test's
//! (`dbus-daemon --session` on a socket of its own), the launcher writes its
//! socket under a runtime directory of the test's (never the user's
//! `$XDG_RUNTIME_DIR/at-spi/bus`), and its settings live in memory
//! (`GSETTINGS_BACKEND=memory`: the launcher records the accessibility switch
//! through GSettings, which would otherwise write the user's dconf database).
//! The daemon, the launcher, the accessibility bus and its registry are one
//! process group, which [`Session`]'s `Drop` ends whatever the test body did.
//!
//! # Ordering
//!
//! Every wait reads observable state — a name on the bus, a node in the
//! tree — in a bounded poll; the pause between two reads only sets how soon
//! the change is noticed.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// How long the bus, its launcher or a node may take to appear.
const APPEAR_LIMIT: Duration = Duration::from_secs(60);

/// The launcher's place differs between distributions.
const LAUNCHERS: [&str; 3] = [
    "/usr/libexec/at-spi-bus-launcher",
    "/usr/lib/at-spi2-core/at-spi-bus-launcher",
    "/usr/lib/at-spi-bus-launcher",
];

/// AT-SPI's role numbers (`AtspiRole`) for the roles `std::ui` publishes.
pub const CHECK_BOX: u32 = 7;
pub const PUSH_BUTTON: u32 = 43;
pub const ENTRY: u32 = 79;

/// One accessible object: where it lives, and what it says it is.
#[derive(Clone, Debug)]
pub struct Accessible {
    pub depth: usize,
    pub role: u32,
    pub name: String,
    bus: String,
    path: String,
}

/// A private session bus with AT-SPI's accessibility bus on it.
pub struct Session {
    daemon: Child,
    /// AT-SPI's launcher, in the daemon's group.
    launcher: Option<Child>,
    /// The session bus, for the program.
    address: String,
    /// The accessibility bus, where the tree is read.
    accessibility: String,
    private: PathBuf,
}

/// Why the tools are missing, if they are: `dbus-daemon`, `gdbus` and AT-SPI's
/// launcher.
pub fn tools_missing() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return Some("AT-SPI is Linux's accessibility bus".to_string());
    }
    for tool in ["dbus-daemon", "gdbus"] {
        let found = Command::new("sh")
            .args(["-c", &format!("command -v {tool}")])
            .stdout(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !found {
            return Some(format!("`{tool}` is not on PATH"));
        }
    }
    if launcher().is_none() {
        return Some("AT-SPI's bus launcher is not installed (at-spi2-core)".to_string());
    }
    None
}

fn launcher() -> Option<&'static str> {
    LAUNCHERS
        .into_iter()
        .find(|path| std::path::Path::new(path).is_file())
}

impl Session {
    /// Starts the bus and the launcher, turns accessibility on, and waits for
    /// the accessibility bus's address.
    pub fn start() -> Session {
        // A short path: a socket's path is limited to about a hundred bytes
        // (`harness_scratch.rs` lists this file for it). Sockets only.
        let private = std::env::temp_dir().join(format!(
            "vilan-atspi-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(private.join("run")).expect("the private directory");
        let mut command = Command::new("dbus-daemon");
        command
            .args(["--session", "--nofork", "--print-address=1"])
            .arg(format!("--address=unix:dir={}", private.display()))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut daemon = command.spawn().expect("start a session bus");
        let mut first = String::new();
        BufReader::new(daemon.stdout.take().expect("piped stdout"))
            .read_line(&mut first)
            .expect("the bus's address");
        let mut session = Session {
            daemon,
            launcher: None,
            address: first.trim().to_string(),
            accessibility: String::new(),
            private,
        };
        // From here a failure drops `session`, which ends the group.
        let mut command = Command::new(launcher().expect("the launcher, checked"));
        command
            .arg("--launch-immediately")
            .envs(session.environment())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(session.daemon.id() as i32);
        }
        // Ended with the group, and reaped, by `Drop`.
        session.launcher = Some(command.spawn().expect("start AT-SPI's bus launcher"));
        session.poll("the launcher on the bus", |session| {
            session
                .session_call(&[
                    "--dest",
                    "org.freedesktop.DBus",
                    "--object-path",
                    "/org/freedesktop/DBus",
                    "--method",
                    "org.freedesktop.DBus.NameHasOwner",
                    "org.a11y.Bus",
                ])
                .is_some_and(|answer| answer.contains("true"))
                .then_some(())
        });
        session
            .session_call(&[
                "--dest",
                "org.a11y.Bus",
                "--object-path",
                "/org/a11y/bus",
                "--method",
                "org.freedesktop.DBus.Properties.Set",
                "org.a11y.Status",
                "IsEnabled",
                "<true>",
            ])
            .expect("turn accessibility on");
        let address = session
            .session_call(&[
                "--dest",
                "org.a11y.Bus",
                "--object-path",
                "/org/a11y/bus",
                "--method",
                "org.a11y.Bus.GetAddress",
            ])
            .expect("the accessibility bus's address");
        session.accessibility = address
            .trim()
            .trim_start_matches("('")
            .trim_end_matches("',)")
            .to_string();
        session
    }

    /// The environment a program runs in to publish on this bus.
    pub fn environment(&self) -> Vec<(String, String)> {
        let private = |name: &str| self.private.join(name).display().to_string();
        vec![
            ("DBUS_SESSION_BUS_ADDRESS".to_string(), self.address.clone()),
            ("XDG_RUNTIME_DIR".to_string(), private("run")),
            ("XDG_CONFIG_HOME".to_string(), private("config")),
            ("XDG_DATA_HOME".to_string(), private("data")),
            ("XDG_CACHE_HOME".to_string(), private("cache")),
            ("GSETTINGS_BACKEND".to_string(), "memory".to_string()),
        ]
    }

    fn session_call(&self, arguments: &[&str]) -> Option<String> {
        let output = Command::new("gdbus")
            .arg("call")
            .arg("--session")
            .args(arguments)
            .envs(self.environment())
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn call(&self, bus: &str, path: &str, method: &str, arguments: &[&str]) -> Option<String> {
        let output = Command::new("gdbus")
            .args(["call", "--address", &self.accessibility, "--dest", bus])
            .args(["--object-path", path, "--method", method])
            .args(arguments)
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Every object of every application on the bus, depth first; `None` when
    /// the tree changed under the walk (an object gone mid-read).
    pub fn tree(&self) -> Option<Vec<Accessible>> {
        let mut found = Vec::new();
        let applications = self.call(
            "org.a11y.atspi.Registry",
            "/org/a11y/atspi/accessible/root",
            "org.a11y.atspi.Accessible.GetChildren",
            &[],
        )?;
        let mut pending: Vec<(String, String, usize)> = references(&applications)
            .into_iter()
            .rev()
            .map(|(bus, path)| (bus, path, 0))
            .collect();
        while let Some((bus, path, depth)) = pending.pop() {
            let role = self.call(&bus, &path, "org.a11y.atspi.Accessible.GetRole", &[])?;
            let role = role
                .trim()
                .trim_start_matches("(uint32 ")
                .trim_end_matches(",)")
                .parse()
                .ok()?;
            let name = self.call(
                &bus,
                &path,
                "org.freedesktop.DBus.Properties.Get",
                &["org.a11y.atspi.Accessible", "Name"],
            )?;
            let name = name
                .trim()
                .trim_start_matches("(<'")
                .trim_end_matches("'>,)")
                .to_string();
            let children = self.call(&bus, &path, "org.a11y.atspi.Accessible.GetChildren", &[])?;
            for (child_bus, child_path) in references(&children).into_iter().rev() {
                pending.push((child_bus, child_path, depth + 1));
            }
            found.push(Accessible {
                depth,
                role,
                name,
                bus,
                path,
            });
        }
        Some(found)
    }

    /// Waits for an object with `role` and `name` to be published with
    /// `interface` among its interfaces (an object's interfaces follow its
    /// node: a text field offers `EditableText` once its text is laid out);
    /// answers it.
    pub fn wait_for(&self, role: u32, name: &str, interface: &str) -> Accessible {
        self.poll(
            &format!("role {role} {name:?} with {interface} in the tree"),
            |session| {
                let object = session
                    .tree()?
                    .into_iter()
                    .find(|object| object.role == role && object.name == name)?;
                let interfaces = session.call(
                    &object.bus,
                    &object.path,
                    "org.a11y.atspi.Accessible.GetInterfaces",
                    &[],
                )?;
                interfaces
                    .contains(&format!("'org.a11y.atspi.{interface}'"))
                    .then_some(object)
            },
        )
    }

    /// The object's action `index` (its default action is 0).
    pub fn act(&self, object: &Accessible, index: u32) -> String {
        self.call(
            &object.bus,
            &object.path,
            "org.a11y.atspi.Action.DoAction",
            &[&index.to_string()],
        )
        .unwrap_or_else(|| panic!("{object:?}'s action {index} failed"))
    }

    /// Replaces an editable object's text.
    pub fn set_text(&self, object: &Accessible, text: &str) -> String {
        self.call(
            &object.bus,
            &object.path,
            "org.a11y.atspi.EditableText.SetTextContents",
            &[text],
        )
        .unwrap_or_else(|| panic!("setting {object:?}'s text failed"))
    }

    fn poll<T>(&self, what: &str, mut read: impl FnMut(&Session) -> Option<T>) -> T {
        let started = Instant::now();
        loop {
            if let Some(found) = read(self) {
                return found;
            }
            assert!(
                started.elapsed() < APPEAR_LIMIT,
                "{what} never appeared; the tree: {:?}",
                self.tree()
            );
            // A bounded poll of state the program is already producing: the
            // interval only sets how soon it is noticed.
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// The process group of the daemon, the launcher and what they started.
    pub fn group(&self) -> u32 {
        self.daemon.id()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let group = self.daemon.id();
        for signal in ["-TERM", "-KILL"] {
            if super::xvfb::live_in_group(group).is_empty() {
                break;
            }
            let _ = Command::new("kill")
                .args([signal, "--", &format!("-{group}")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                let _ = self.daemon.try_wait();
                if let Some(launcher) = &mut self.launcher {
                    let _ = launcher.try_wait();
                }
                if super::xvfb::live_in_group(group).is_empty() {
                    break;
                }
                // Processes ALREADY signalled to end: the interval only sets
                // how soon their end is noticed.
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        let _ = self.daemon.try_wait();
        if let Some(launcher) = &mut self.launcher {
            let _ = launcher.try_wait();
        }
        let _ = std::fs::remove_dir_all(&self.private);
    }
}

/// The `(bus, path)` references of a `GetChildren` answer. GVariant's text
/// form types only the first element (`objectpath '…'`).
fn references(answer: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut rest = answer;
    while let Some(open) = rest.find("('") {
        rest = &rest[open + 2..];
        let Some(bus_end) = rest.find('\'') else {
            break;
        };
        let bus = rest[..bus_end].to_string();
        rest = &rest[bus_end + 1..];
        let Some(path_start) = rest.find('\'') else {
            break;
        };
        rest = &rest[path_start + 1..];
        let Some(path_end) = rest.find('\'') else {
            break;
        };
        found.push((bus, rest[..path_end].to_string()));
        rest = &rest[path_end + 1..];
    }
    found
}
