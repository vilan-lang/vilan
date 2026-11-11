//! Real pointer input for an end-to-end test, through the X server's XTEST
//! extension: the events arrive at the window exactly as a mouse's do (core
//! and XInput2 both), which a synthetic `SendEvent` would not.
//!
//!     x11_input --title "vilan gfx" move 180 164 press release close
//!
//! The window is found by its title among the root window's children; each
//! `move X Y` puts the pointer at device pixel `(X, Y)` of the window's
//! inside, `press` and `release` are the left button, and `close` sends the
//! WM_DELETE_WINDOW message a window manager sends for its close button. The
//! commands run in order, and the program ends after a round trip, so the
//! server has handled every one of them when it exits. The display is
//! `DISPLAY` (and `XAUTHORITY`), as for any X client.
//!
//! Used by the compiler's `std::gfx` end-to-end test
//! (`crates/vilan-cli/tests/gfx_window.rs`), which builds it from here.

#[cfg(target_os = "linux")]
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = x11::run(&arguments) {
        eprintln!("x11_input: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("x11_input: X11 input is Linux-only here");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
mod x11 {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        Atom, AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, MapState, Window,
    };
    use x11rb::protocol::xtest::ConnectionExt as _;

    type Failure = Box<dyn std::error::Error>;

    const BUTTON_PRESS: u8 = 4;
    const BUTTON_RELEASE: u8 = 5;
    const MOTION_NOTIFY: u8 = 6;
    const LEFT_BUTTON: u8 = 1;

    pub fn run(arguments: &[String]) -> Result<(), Failure> {
        let (connection, screen) = x11rb::connect(None)?;
        let root = connection.setup().roots[screen].root;
        let mut words = arguments.iter();
        let mut title = None;
        let mut commands = Vec::new();
        while let Some(word) = words.next() {
            match word.as_str() {
                "--title" => title = Some(words.next().ok_or("--title needs a value")?.clone()),
                "move" => {
                    let x: i16 = words.next().ok_or("move needs X")?.parse()?;
                    let y: i16 = words.next().ok_or("move needs Y")?.parse()?;
                    commands.push(Command::Move(x, y));
                }
                "press" => commands.push(Command::Press),
                "release" => commands.push(Command::Release),
                "close" => commands.push(Command::Close),
                other => return Err(format!("unknown command {other:?}").into()),
            }
        }
        let title = title.ok_or("--title names the window")?;
        let window = find_window(&connection, root, &title)?;
        for command in commands {
            match command {
                Command::Move(x, y) => {
                    let at = connection
                        .translate_coordinates(window, root, x, y)?
                        .reply()?;
                    connection.xtest_fake_input(
                        MOTION_NOTIFY,
                        0,
                        x11rb::CURRENT_TIME,
                        root,
                        at.dst_x,
                        at.dst_y,
                        0,
                    )?;
                }
                Command::Press | Command::Release => {
                    let kind = if matches!(command, Command::Press) {
                        BUTTON_PRESS
                    } else {
                        BUTTON_RELEASE
                    };
                    connection.xtest_fake_input(
                        kind,
                        LEFT_BUTTON,
                        x11rb::CURRENT_TIME,
                        x11rb::NONE,
                        0,
                        0,
                        0,
                    )?;
                }
                Command::Close => {
                    let protocols = atom(&connection, b"WM_PROTOCOLS")?;
                    let delete_window = atom(&connection, b"WM_DELETE_WINDOW")?;
                    let message =
                        ClientMessageEvent::new(32, window, protocols, [delete_window, 0, 0, 0, 0]);
                    connection.send_event(false, window, EventMask::NO_EVENT, message)?;
                }
            }
            // A round trip per command: each is handled before the next.
            connection.get_input_focus()?.reply()?;
        }
        Ok(())
    }

    enum Command {
        Move(i16, i16),
        Press,
        Release,
        Close,
    }

    fn atom(connection: &impl Connection, name: &[u8]) -> Result<Atom, Failure> {
        Ok(connection.intern_atom(false, name)?.reply()?.atom)
    }

    /// The viewable child of the root window whose title is `title`.
    fn find_window(
        connection: &impl Connection,
        root: Window,
        title: &str,
    ) -> Result<Window, Failure> {
        let net_name = atom(connection, b"_NET_WM_NAME")?;
        let utf8 = atom(connection, b"UTF8_STRING")?;
        for child in connection.query_tree(root)?.reply()?.children {
            if connection.get_window_attributes(child)?.reply()?.map_state != MapState::VIEWABLE {
                continue;
            }
            for (property, kind) in [
                (net_name, utf8),
                (AtomEnum::WM_NAME.into(), AtomEnum::STRING.into()),
            ] {
                let named = connection
                    .get_property(false, child, property, kind, 0, 1024)?
                    .reply()?;
                if named.value == title.as_bytes() {
                    return Ok(child);
                }
            }
        }
        Err(format!("no viewable window titled {title:?}").into())
    }
}
