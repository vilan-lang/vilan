//! Window input as the vilan program receives it: each `winit` event the
//! program can see becomes a [`RawEvent`] in dp, queued on its window's
//! source and decoded by `std::gfx` (`gfx.vl`) through three externs.
//!
//! # The seam
//!
//! An event crosses as a KIND code, up to [`NUMBERS`] numbers and one string,
//! read one at a time (`__gfx_event_next`, `__gfx_event_number`,
//! `__gfx_event_text`), so nothing the runtime allocates is a vilan type and
//! nothing is copied that the program does not read. ONE table, kept by
//! `gfx.vl`'s decoder too:
//!
//! | kind | event | numbers | text |
//! |---|---|---|---|
//! | 0 | pointer moved | x, y | |
//! | 1 | pointer pressed | x, y, button, modifiers, time | |
//! | 2 | pointer released | x, y, button, modifiers | |
//! | 3 | pointer left | | |
//! | 4 | scrolled | x, y, delta x, delta y, modifiers | |
//! | 5 | key pressed | repeat (0 or 1), modifiers, named (0 or 1) | the key |
//! | 6 | key released | repeat (0 or 1), modifiers, named (0 or 1) | the key |
//! | 7 | text input | | the text |
//! | 8 | resized | width, height | |
//! | 9 | scale changed | scale factor | |
//! | 10 | close requested | | |
//! | 11 | focus changed | focused (0 or 1) | |
//!
//! Positions and sizes are dp (device pixels divided by the window's content
//! scale, the same scale its frames are drawn at). A button is 0 left, 1
//! right, 2 middle, 3 back, 4 forward, `5 + n` for winit's `Other(n)`.
//! A press's time is milliseconds since the host started, stamped when the
//! window system delivered it (presses in a row are counted by it); a press
//! the program delivers itself (`std::ui::deliver`) carries 0.
//! Modifiers are bits: 1 shift, 2 control, 4 alt, 8 meta (the Windows or
//! Command key). A key is named as the web's `KeyboardEvent.key` names it:
//! the character it types (`"a"`, `"A"`, `"1"`, and `" "` for the space bar),
//! `named` 0; or the name of a key that types none (`"Enter"`, `"ArrowUp"`,
//! `"Escape"`), winit's `NamedKey` spelled as written, `named` 1.

use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta};
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// The most numbers any event carries.
pub(crate) const NUMBERS: usize = 5;

/// Scroll by LINES (a mouse wheel's notches) arrives as dp at this many dp
/// per line, so a program reads one unit whatever the device.
pub(crate) const DP_PER_SCROLL_LINE: f64 = 32.0;

pub(crate) const POINTER_MOVED: i32 = 0;
pub(crate) const POINTER_PRESSED: i32 = 1;
pub(crate) const POINTER_RELEASED: i32 = 2;
pub(crate) const POINTER_LEFT: i32 = 3;
pub(crate) const SCROLLED: i32 = 4;
pub(crate) const KEY_PRESSED: i32 = 5;
pub(crate) const KEY_RELEASED: i32 = 6;
pub(crate) const TEXT_INPUT: i32 = 7;
pub(crate) const RESIZED: i32 = 8;
pub(crate) const SCALE_CHANGED: i32 = 9;
pub(crate) const CLOSE_REQUESTED: i32 = 10;
pub(crate) const FOCUS_CHANGED: i32 = 11;

/// One event, ready to cross.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RawEvent {
    pub(crate) kind: i32,
    pub(crate) numbers: [f64; NUMBERS],
    pub(crate) text: Option<String>,
}

impl RawEvent {
    fn new(kind: i32, numbers: &[f64]) -> RawEvent {
        let mut all = [0.0; NUMBERS];
        all[..numbers.len()].copy_from_slice(numbers);
        RawEvent {
            kind,
            numbers: all,
            text: None,
        }
    }

    pub(crate) fn pointer_moved(x: f64, y: f64) -> RawEvent {
        RawEvent::new(POINTER_MOVED, &[x, y])
    }

    /// `time`: a press's, in milliseconds ([`since_start`]); a release
    /// carries none.
    pub(crate) fn pointer_button(
        state: ElementState,
        x: f64,
        y: f64,
        button: MouseButton,
        modifiers: ModifiersState,
        time: f64,
    ) -> RawEvent {
        match state {
            ElementState::Pressed => RawEvent::new(
                POINTER_PRESSED,
                &[x, y, button_code(button), modifier_bits(modifiers), time],
            ),
            ElementState::Released => RawEvent::new(
                POINTER_RELEASED,
                &[x, y, button_code(button), modifier_bits(modifiers)],
            ),
        }
    }

    pub(crate) fn pointer_left() -> RawEvent {
        RawEvent::new(POINTER_LEFT, &[])
    }

    /// `delta` in device pixels (or lines) as winit reports it; `content_scale`
    /// turns pixels into dp.
    pub(crate) fn scrolled(
        x: f64,
        y: f64,
        delta: MouseScrollDelta,
        content_scale: f64,
        modifiers: ModifiersState,
    ) -> RawEvent {
        let (delta_x, delta_y) = match delta {
            MouseScrollDelta::LineDelta(columns, lines) => (
                f64::from(columns) * DP_PER_SCROLL_LINE,
                f64::from(lines) * DP_PER_SCROLL_LINE,
            ),
            MouseScrollDelta::PixelDelta(position) => {
                (position.x / content_scale, position.y / content_scale)
            }
        };
        RawEvent::new(
            SCROLLED,
            &[x, y, delta_x, delta_y, modifier_bits(modifiers)],
        )
    }

    /// A key, and — for a press that types something — the text it types,
    /// as a second event. Control characters (Enter's `\r`, Backspace,
    /// Tab, Escape) type nothing a text field would insert: those keys
    /// arrive as named keys only.
    pub(crate) fn key(event: &KeyEvent, modifiers: ModifiersState) -> Vec<RawEvent> {
        let kind = match event.state {
            ElementState::Pressed => KEY_PRESSED,
            ElementState::Released => KEY_RELEASED,
        };
        let (name, named) = key_name(&event.logical_key);
        let mut key = RawEvent::new(
            kind,
            &[
                f64::from(u8::from(event.repeat)),
                modifier_bits(modifiers),
                f64::from(u8::from(named)),
            ],
        );
        key.text = Some(name);
        let mut events = vec![key];
        if event.state == ElementState::Pressed
            && let Some(text) = &event.text
            && !text.is_empty()
            && !text.chars().any(char::is_control)
        {
            events.push(RawEvent::text_input(text.to_string()));
        }
        events
    }

    pub(crate) fn text_input(text: String) -> RawEvent {
        let mut event = RawEvent::new(TEXT_INPUT, &[]);
        event.text = Some(text);
        event
    }

    pub(crate) fn resized(width: f64, height: f64) -> RawEvent {
        RawEvent::new(RESIZED, &[width, height])
    }

    pub(crate) fn scale_changed(scale_factor: f64) -> RawEvent {
        RawEvent::new(SCALE_CHANGED, &[scale_factor])
    }

    pub(crate) fn close_requested() -> RawEvent {
        RawEvent::new(CLOSE_REQUESTED, &[])
    }

    /// The window gained (`true`) or lost the system's keyboard focus.
    pub(crate) fn focus_changed(focused: bool) -> RawEvent {
        RawEvent::new(FOCUS_CHANGED, &[f64::from(u8::from(focused))])
    }
}

/// Milliseconds since the host first asked: a press's time.
pub(crate) fn since_start() -> f64 {
    host_clock().as_secs_f64() * 1000.0
}

/// The host's ONE clock: the time since it was first read. A press's time
/// ([`since_start`]) and an element timer's due time (`ui/timers.rs`) are
/// both read from it, and the tree is handed its readings, never the clock.
pub(crate) fn host_clock() -> std::time::Duration {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed()
}

fn button_code(button: MouseButton) -> f64 {
    match button {
        MouseButton::Left => 0.0,
        MouseButton::Right => 1.0,
        MouseButton::Middle => 2.0,
        MouseButton::Back => 3.0,
        MouseButton::Forward => 4.0,
        MouseButton::Other(code) => 5.0 + f64::from(code),
    }
}

fn modifier_bits(modifiers: ModifiersState) -> f64 {
    let mut bits = 0u8;
    if modifiers.shift_key() {
        bits |= 1;
    }
    if modifiers.control_key() {
        bits |= 2;
    }
    if modifiers.alt_key() {
        bits |= 4;
    }
    if modifiers.super_key() {
        bits |= 8;
    }
    f64::from(bits)
}

/// The web's `KeyboardEvent.key` for a key — what it types, or its name —
/// and whether it is a name. The space bar types a space: winit names it,
/// the web and a terminal both give it as `" "`.
fn key_name(key: &Key) -> (String, bool) {
    match key {
        Key::Character(text) => (text.to_string(), false),
        Key::Named(NamedKey::Space) => (" ".to_string(), false),
        Key::Named(named) => (format!("{named:?}"), true),
        Key::Dead(_) => ("Dead".to_string(), true),
        Key::Unidentified(_) => ("Unidentified".to_string(), true),
    }
}

/// Adds `event` to `queue`, folding a pointer move into a pointer move that
/// is still waiting: the program reads where the pointer IS, and a burst of
/// motion between two turns is one move, not dozens.
pub(crate) fn enqueue(queue: &mut std::collections::VecDeque<RawEvent>, event: RawEvent) {
    if event.kind == POINTER_MOVED
        && let Some(last) = queue.back_mut()
        && last.kind == POINTER_MOVED
    {
        *last = event;
        return;
    }
    queue.push_back(event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_of_pointer_motion_is_one_move_and_other_events_keep_their_order() {
        let mut queue = std::collections::VecDeque::new();
        enqueue(&mut queue, RawEvent::pointer_moved(1.0, 1.0));
        enqueue(&mut queue, RawEvent::pointer_moved(2.0, 2.0));
        enqueue(
            &mut queue,
            RawEvent::pointer_button(
                ElementState::Pressed,
                2.0,
                2.0,
                MouseButton::Left,
                ModifiersState::empty(),
                0.0,
            ),
        );
        enqueue(&mut queue, RawEvent::pointer_moved(3.0, 3.0));
        enqueue(&mut queue, RawEvent::pointer_moved(4.0, 5.0));
        let kinds: Vec<(i32, f64, f64)> = queue
            .iter()
            .map(|event| (event.kind, event.numbers[0], event.numbers[1]))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (POINTER_MOVED, 2.0, 2.0),
                (POINTER_PRESSED, 2.0, 2.0),
                (POINTER_MOVED, 4.0, 5.0)
            ]
        );
    }

    #[test]
    fn buttons_and_modifiers_cross_as_the_table_says() {
        let modifiers = ModifiersState::SHIFT | ModifiersState::SUPER;
        let event = RawEvent::pointer_button(
            ElementState::Released,
            10.0,
            20.0,
            MouseButton::Other(3),
            modifiers,
            1234.0,
        );
        assert_eq!(event.kind, POINTER_RELEASED);
        assert_eq!(
            event.numbers,
            [10.0, 20.0, 8.0, 9.0, 0.0],
            "a release has no time"
        );
        let press = RawEvent::pointer_button(
            ElementState::Pressed,
            10.0,
            20.0,
            MouseButton::Left,
            modifiers,
            1234.0,
        );
        assert_eq!(press.kind, POINTER_PRESSED);
        assert_eq!(press.numbers, [10.0, 20.0, 0.0, 9.0, 1234.0]);
    }

    #[test]
    fn wheel_lines_and_touchpad_pixels_both_arrive_in_dp() {
        let lines = RawEvent::scrolled(
            0.0,
            0.0,
            MouseScrollDelta::LineDelta(0.0, -2.0),
            1.5,
            ModifiersState::empty(),
        );
        assert_eq!(lines.numbers[2..4], [0.0, -2.0 * DP_PER_SCROLL_LINE]);
        let pixels = RawEvent::scrolled(
            0.0,
            0.0,
            MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(30.0, 15.0)),
            1.5,
            ModifiersState::empty(),
        );
        assert_eq!(pixels.numbers[2..4], [20.0, 10.0]);
    }

    #[test]
    fn keys_are_named_as_the_web_names_them() {
        assert_eq!(
            key_name(&Key::Named(NamedKey::ArrowUp)),
            ("ArrowUp".to_string(), true)
        );
        assert_eq!(
            key_name(&Key::Named(NamedKey::Enter)),
            ("Enter".to_string(), true)
        );
        assert_eq!(
            key_name(&Key::Named(NamedKey::F12)),
            ("F12".to_string(), true)
        );
        assert_eq!(
            key_name(&Key::Character("a".into())),
            ("a".to_string(), false)
        );
        assert_eq!(
            key_name(&Key::Named(NamedKey::Space)),
            (" ".to_string(), false)
        );
    }
}
