//! Real pointer input for an end-to-end test, through the X server's XTEST
//! extension: the events arrive at the window exactly as a mouse's do (core
//! and XInput2 both), which a synthetic `SendEvent` would not.
//!
//!     x11_input --title "vilan gfx" move 180 164 press release close
//!     x11_input --title "vilan ui list" wheel down 3 key Tab key shift+Tab
//!
//! The window is found by its title among the root window's children; each
//! `move X Y` puts the pointer at device pixel `(X, Y)` of the window's
//! inside, `press` and `release` are the left button, `wheel up N` and
//! `wheel down N` turn the wheel N notches (buttons 4 and 5, pressed and
//! released), `key NAME` presses and releases the key whose X keysym is
//! `NAME` (`Tab`, `Return`, `space`, `a`), with `shift+` in front holding
//! Shift around it, and `close` sends the WM_DELETE_WINDOW message a window
//! manager sends for its close button. Keys go to the window: it is given
//! the input focus first, as a window manager gives it on a click. The
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
        Atom, AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, InputFocus, Keycode,
        MapState, Window,
    };
    use x11rb::protocol::xtest::ConnectionExt as _;

    type Failure = Box<dyn std::error::Error>;

    const KEY_PRESS: u8 = 2;
    const KEY_RELEASE: u8 = 3;
    const BUTTON_PRESS: u8 = 4;
    const BUTTON_RELEASE: u8 = 5;
    const MOTION_NOTIFY: u8 = 6;
    const LEFT_BUTTON: u8 = 1;
    const WHEEL_UP: u8 = 4;
    const WHEEL_DOWN: u8 = 5;

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
                "wheel" => {
                    let button = match words.next().map(String::as_str) {
                        Some("up") => WHEEL_UP,
                        Some("down") => WHEEL_DOWN,
                        other => return Err(format!("wheel up or down, not {other:?}").into()),
                    };
                    let notches: u32 = words.next().ok_or("wheel needs a count")?.parse()?;
                    commands.push(Command::Wheel(button, notches));
                }
                "key" => {
                    let name = words.next().ok_or("key needs a keysym name")?;
                    let (shift, name) = match name.strip_prefix("shift+") {
                        Some(name) => (true, name),
                        None => (false, name.as_str()),
                    };
                    commands.push(Command::Key(shift, keysym(name)?));
                }
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
                Command::Wheel(button, notches) => {
                    for _ in 0..notches {
                        for kind in [BUTTON_PRESS, BUTTON_RELEASE] {
                            connection.xtest_fake_input(
                                kind,
                                button,
                                x11rb::CURRENT_TIME,
                                x11rb::NONE,
                                0,
                                0,
                                0,
                            )?;
                        }
                        // One notch handled before the next: each is its own
                        // wheel event.
                        connection.get_input_focus()?.reply()?;
                    }
                }
                Command::Key(shift, symbol) => {
                    connection.set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME)?;
                    let code = keycode(&connection, symbol)?;
                    let shift_code = keycode(&connection, SHIFT_L)?;
                    let mut sequence = Vec::new();
                    if shift {
                        sequence.push((KEY_PRESS, shift_code));
                    }
                    sequence.push((KEY_PRESS, code));
                    sequence.push((KEY_RELEASE, code));
                    if shift {
                        sequence.push((KEY_RELEASE, shift_code));
                    }
                    for (kind, code) in sequence {
                        connection.xtest_fake_input(
                            kind,
                            code,
                            x11rb::CURRENT_TIME,
                            x11rb::NONE,
                            0,
                            0,
                            0,
                        )?;
                    }
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
        /// A wheel button and how many notches.
        Wheel(u8, u32),
        /// Whether Shift is held, and the key's keysym.
        Key(bool, u32),
        Close,
    }

    const SHIFT_L: u32 = 0xffe1;

    /// The keysyms the tests name: X's own spelling.
    fn keysym(name: &str) -> Result<u32, Failure> {
        Ok(match name {
            "Tab" => 0xff09,
            "Return" => 0xff0d,
            "Escape" => 0xff1b,
            "space" => 0x20,
            single if single.len() == 1 && single.is_ascii() => u32::from(single.as_bytes()[0]),
            other => return Err(format!("no keysym {other:?} here").into()),
        })
    }

    /// The keycode the server's keyboard map gives `symbol`.
    fn keycode(connection: &impl Connection, symbol: u32) -> Result<Keycode, Failure> {
        let setup = connection.setup();
        let (first, last) = (setup.min_keycode, setup.max_keycode);
        let mapping = connection
            .get_keyboard_mapping(first, last - first + 1)?
            .reply()?;
        let per = usize::from(mapping.keysyms_per_keycode);
        mapping
            .keysyms
            .chunks(per.max(1))
            .position(|symbols| symbols.contains(&symbol))
            .map(|index| first + index as u8)
            .ok_or_else(|| format!("no key types keysym {symbol:#x}").into())
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
