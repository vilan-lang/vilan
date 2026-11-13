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
//! released — and one notch scrolls TWO lines, 64 dp, in the window: XTEST's
//! device has no scroll valuator, so its wheel buttons are not flagged as
//! emulated, and `winit` 0.30's X11 backend turns both the press AND the
//! release of an unflagged button 4 to 7 into a `MouseWheel` of one line; a
//! real wheel on an XInput2 server scrolls by its valuator and its button
//! events ARE flagged, so it moves 32 dp a notch), `key NAME` presses and releases the key whose X keysym is
//! `NAME` (`Tab`, `Return`, `Up`, `space`, `a`), with `shift+` in front holding
//! Shift around it, and `close` sends the WM_DELETE_WINDOW message a window
//! manager sends for its close button. Keys go to the window: it is given
//! the input focus first, as a window manager gives it on a click. The
//! commands run in order, and the program ends after a round trip, so the
//! server has handled every one of them when it exits. The display is
//! `DISPLAY` (and `XAUTHORITY`), as for any X client.
//!
//!     x11_input clipboard read
//!     x11_input clipboard own "some text"
//!
//! reads or owns the CLIPBOARD selection as another X client does (the
//! `clipboard` module below; no window is needed, and `--title` is ignored).
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

    pub type Failure = Box<dyn std::error::Error>;

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
                "clipboard" => {
                    return super::clipboard::run(&connection, root, words.as_slice());
                }
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
            "Up" => 0xff52,
            "Down" => 0xff54,
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

    pub fn atom(connection: &impl Connection, name: &[u8]) -> Result<Atom, Failure> {
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

/// `clipboard read` and `clipboard own TEXT`: the CLIPBOARD selection as
/// another X client sees it, for the `std::gfx` clipboard's end-to-end test
/// (F173). `read` asks the selection's owner for its text as `UTF8_STRING`
/// and prints `clipboard TEXT`, or `clipboard none` when nobody owns it or
/// the owner has no text. `own TEXT` takes the selection, prints `owned`, and
/// serves `TEXT` (and the `TARGETS` it offers) until it has handed the text
/// over once, then exits. Each gives up after half a minute with an error,
/// so a peer that never answers fails the test instead of hanging it. Only
/// ever run under `xvfb-run`: the selection is that private server's.
#[cfg(target_os = "linux")]
mod clipboard {
    use std::io::Write;
    use std::time::Duration;

    use x11rb::connection::Connection;
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::{
        Atom, AtomEnum, ConnectionExt, CreateWindowAux, EventMask, PropMode,
        SELECTION_NOTIFY_EVENT, SelectionNotifyEvent, Window, WindowClass,
    };
    use x11rb::wrapper::ConnectionExt as _;

    use super::x11::{Failure, atom};

    /// How long a run may wait for its peer before it is a failure.
    const LIMIT: Duration = Duration::from_secs(30);

    pub fn run(
        connection: &impl Connection,
        root: Window,
        words: &[String],
    ) -> Result<(), Failure> {
        std::thread::spawn(|| {
            std::thread::sleep(LIMIT);
            eprintln!("x11_input: the clipboard's peer did not answer in {LIMIT:?}");
            std::process::exit(2);
        });
        let window = connection.generate_id()?;
        connection.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        let atoms = Atoms {
            clipboard: atom(connection, b"CLIPBOARD")?,
            utf8: atom(connection, b"UTF8_STRING")?,
            targets: atom(connection, b"TARGETS")?,
            property: atom(connection, b"VILAN_X11_INPUT_PASTE")?,
        };
        match words {
            [read] if read == "read" => self::read(connection, window, &atoms),
            [own, text] if own == "own" => self::own(connection, window, &atoms, text),
            other => Err(format!("clipboard read, or clipboard own TEXT, not {other:?}").into()),
        }
    }

    struct Atoms {
        clipboard: Atom,
        utf8: Atom,
        targets: Atom,
        property: Atom,
    }

    fn read(connection: &impl Connection, window: Window, atoms: &Atoms) -> Result<(), Failure> {
        connection.convert_selection(
            window,
            atoms.clipboard,
            atoms.utf8,
            atoms.property,
            x11rb::CURRENT_TIME,
        )?;
        connection.flush()?;
        loop {
            if let Event::SelectionNotify(notify) = connection.wait_for_event()?
                && notify.requestor == window
            {
                if notify.property == x11rb::NONE {
                    println!("clipboard none");
                } else {
                    let reply = connection
                        .get_property(true, window, atoms.property, AtomEnum::ANY, 0, 1 << 20)?
                        .reply()?;
                    println!("clipboard {}", String::from_utf8_lossy(&reply.value));
                }
                return Ok(());
            }
        }
    }

    fn own(
        connection: &impl Connection,
        window: Window,
        atoms: &Atoms,
        text: &str,
    ) -> Result<(), Failure> {
        connection.set_selection_owner(window, atoms.clipboard, x11rb::CURRENT_TIME)?;
        if connection
            .get_selection_owner(atoms.clipboard)?
            .reply()?
            .owner
            != window
        {
            return Err("the server did not give this client the clipboard".into());
        }
        println!("owned");
        std::io::stdout().flush()?;
        loop {
            match connection.wait_for_event()? {
                Event::SelectionRequest(request) => {
                    // An obsolete requestor names no property: the target is it.
                    let property = if request.property == x11rb::NONE {
                        request.target
                    } else {
                        request.property
                    };
                    let mut served = false;
                    let answered = if request.target == atoms.targets {
                        connection.change_property32(
                            PropMode::REPLACE,
                            request.requestor,
                            property,
                            AtomEnum::ATOM,
                            &[atoms.targets, atoms.utf8],
                        )?;
                        property
                    } else if request.target == atoms.utf8 {
                        connection.change_property8(
                            PropMode::REPLACE,
                            request.requestor,
                            property,
                            atoms.utf8,
                            text.as_bytes(),
                        )?;
                        served = true;
                        property
                    } else {
                        x11rb::NONE
                    };
                    let notify = SelectionNotifyEvent {
                        response_type: SELECTION_NOTIFY_EVENT,
                        sequence: 0,
                        time: request.time,
                        requestor: request.requestor,
                        selection: request.selection,
                        target: request.target,
                        property: answered,
                    };
                    connection.send_event(false, request.requestor, EventMask::NO_EVENT, notify)?;
                    // A round trip: the server has handled the property and the
                    // notice before this client can exit. Leaving straight
                    // after a flush lost them in about half of the runs.
                    connection.get_input_focus()?.reply()?;
                    if served {
                        return Ok(());
                    }
                }
                Event::SelectionClear(_) => {
                    return Err("another client took the clipboard before it was read".into());
                }
                _ => {}
            }
        }
    }
}
