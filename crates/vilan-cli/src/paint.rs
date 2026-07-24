//! Coloring for the CLI's output — the `Compiled …`, `[watch] …`, `hmr: …`,
//! `error:`/`warning:` prefixes, and the test-runner summary, which this module
//! dresses directly; and the ariadne diagnostics, which render themselves but
//! now take their color/no-color verdict from here (see below).
//!
//! Hand-rolled ANSI rather than ariadne's `Color`: ariadne's `Fmt` emits codes
//! unconditionally (it leaves the terminal check to the `Report` renderer), so
//! reusing it would mean either driving yansi's global enable state or always
//! allocating a styled wrapper. A handful of SGR constants gate explicitly, once
//! per stream, and hand back the input unchanged on the plain path.
//!
//! Gating (both must hold for a stream to be colored): the stream is a terminal
//! (`IsTerminal`, checked on stdout and stderr separately) **and** `NO_COLOR` is
//! unset or empty (no-color.org — any non-empty value disables). A piped or
//! redirected stream stays byte-for-byte plain, which is what the e2e suite reads.
//!
//! The gate is also the CLI's single source of truth for whether **ariadne**
//! colors a diagnostic (`windows-support.md` §6): the `Report` config takes
//! [`stderr_enabled`], so a compiler error obeys the same TTY + `NO_COLOR` rule
//! as every other line.
//!
//! On Windows, deciding a stream is colored additionally turns on
//! `ENABLE_VIRTUAL_TERMINAL_PROCESSING` for its console, so the escapes render
//! on a legacy conhost and not just in Windows Terminal. That is the *only*
//! Windows-specific behavior here — the decision itself (`is_terminal &&
//! !no_color`) is untouched, and a stream that is not a console never reaches
//! the call because the gate already said "not a terminal".

use std::borrow::Cow;
use std::io::IsTerminal;
use std::sync::OnceLock;

/// An ANSI SGR style as its parameter body: `"32"` is green, `"1;31"` bold red.
#[derive(Clone, Copy)]
pub struct Style(&'static str);

impl Style {
    pub const GREEN: Style = Style("32");
    pub const YELLOW: Style = Style("33");
    pub const CYAN: Style = Style("36");
    pub const BOLD: Style = Style("1");
    pub const DIM: Style = Style("2");
    pub const BOLD_RED: Style = Style("1;31");
    pub const BOLD_GREEN: Style = Style("1;32");
    pub const BOLD_YELLOW: Style = Style("1;33");
}

/// Wraps `text` in `style`'s SGR codes when `enabled`; otherwise hands it back
/// borrowed and byte-identical — the plain path allocates nothing and never
/// reformats. Kept pure (the flag is a parameter) so the pins exercise both arms
/// without a real terminal.
fn wrap(enabled: bool, style: Style, text: &str) -> Cow<'_, str> {
    if enabled {
        Cow::Owned(format!("\x1b[{}m{}\x1b[0m", style.0, text))
    } else {
        Cow::Borrowed(text)
    }
}

/// The color gate for one stream: paint only a real terminal, and only when
/// `NO_COLOR` permits it. Both inputs are parameters, so the rule is pinned off a
/// TTY — and `NO_COLOR` winning over a terminal is just `is_terminal && !no_color`.
fn gate(is_terminal: bool, no_color: bool) -> bool {
    is_terminal && !no_color
}

/// `NO_COLOR` is honored when present and non-empty (no-color.org).
fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
}

/// Which of the two standard streams a gate decision is about — the parameter
/// `enable_virtual_terminal` needs to pick a console handle on Windows.
#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

/// Turns on `ENABLE_VIRTUAL_TERMINAL_PROCESSING` for `stream`'s console, so ANSI
/// escapes render on a legacy conhost. Called only for a stream the gate has
/// already decided *is* a colored terminal; a failure (no console, a mode the
/// OS refuses) is silent — the alternative would be to un-decide a gate that is
/// correct on Windows Terminal, which enables VT for us.
#[cfg(windows)]
fn enable_virtual_terminal(stream: Stream) {
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_ERROR_HANDLE,
        STD_OUTPUT_HANDLE, SetConsoleMode,
    };
    let id = match stream {
        Stream::Stdout => STD_OUTPUT_HANDLE,
        Stream::Stderr => STD_ERROR_HANDLE,
    };
    unsafe {
        let handle = GetStdHandle(id);
        if handle.is_null() {
            return;
        }
        let mut mode = 0u32;
        if GetConsoleMode(handle, &raw mut mode) == 0 {
            return;
        }
        SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
    }
}

#[cfg(not(windows))]
fn enable_virtual_terminal(_stream: Stream) {}

/// The gate decision for one stream, plus the Windows console setup a "yes"
/// implies. Split out so both streams share one shape.
fn decide(stream: Stream, is_terminal: bool) -> bool {
    let enabled = gate(is_terminal, no_color());
    if enabled {
        enable_virtual_terminal(stream);
    }
    enabled
}

// Each stream's verdict is computed once, on first paint — a build or watch run
// dresses many lines but probes the terminal (and `NO_COLOR`) a single time.
fn stdout_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| decide(Stream::Stdout, std::io::stdout().is_terminal()))
}

/// Whether stderr may carry ANSI — also what `main.rs` hands ariadne, so a
/// diagnostic and a status line can never disagree about the terminal.
pub fn stderr_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| decide(Stream::Stderr, std::io::stderr().is_terminal()))
}

/// Paint `text` for a stdout line (`println!`), gated on stdout being a terminal.
pub fn out(style: Style, text: &str) -> Cow<'_, str> {
    wrap(stdout_enabled(), style, text)
}

/// Paint `text` for a stderr line (`eprintln!`), gated on stderr being a terminal.
pub fn err(style: Style, text: &str) -> Cow<'_, str> {
    wrap(stderr_enabled(), style, text)
}

/// The shared `error:` prefix (red + bold on a terminal, the plain literal when
/// piped) that opens every CLI error line outside the ariadne diagnostic path.
pub fn error_prefix() -> Cow<'static, str> {
    err(Style::BOLD_RED, "error:")
}

/// The shared `warning:` prefix (yellow + bold on a terminal). Bold for parity
/// with `error:` — a colored prefix reads as one unit.
pub fn warning_prefix() -> Cow<'static, str> {
    err(Style::BOLD_YELLOW, "warning:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_enabled_stream_paints_with_the_style_code() {
        assert_eq!(wrap(true, Style::GREEN, "ok"), "\x1b[32mok\x1b[0m");
    }

    #[test]
    fn a_disabled_stream_is_zero_alloc_byte_identical_passthrough() {
        let painted = wrap(false, Style::GREEN, "Compiled a -> b");
        assert_eq!(painted, "Compiled a -> b");
        // Borrowed, not a fresh String: the plain path costs no allocation and
        // hands back the exact bytes a pipe must see.
        assert!(matches!(painted, Cow::Borrowed(_)));
    }

    #[test]
    fn no_color_wins_over_a_terminal() {
        // The gate: NO_COLOR set (true) beats a terminal (true) → no color.
        assert!(!gate(true, true));
        // ...and end to end, the string comes out plain.
        assert_eq!(wrap(gate(true, true), Style::BOLD_RED, "error:"), "error:");
    }

    #[test]
    fn a_terminal_without_no_color_paints() {
        assert!(gate(true, false));
    }

    #[test]
    fn a_non_terminal_never_paints() {
        // Piped/redirected: plain regardless of NO_COLOR.
        assert!(!gate(false, false));
        assert!(!gate(false, true));
    }

    #[test]
    fn the_decision_adds_no_policy_to_the_gate() {
        // `decide` exists to hang the Windows console setup off a "yes"; the
        // verdict itself must stay exactly `gate(is_terminal, no_color())`
        // (windows-support.md §6 changes consumers, never the rule).
        assert_eq!(decide(Stream::Stdout, false), gate(false, no_color()));
        assert_eq!(decide(Stream::Stderr, false), gate(false, no_color()));
        assert_eq!(decide(Stream::Stdout, true), gate(true, no_color()));
        assert_eq!(decide(Stream::Stderr, true), gate(true, no_color()));
    }

    #[test]
    fn a_bold_colored_prefix_composes_both_sgr_codes() {
        // The error/warning prefixes lean on two-parameter styles.
        assert_eq!(
            wrap(true, Style::BOLD_RED, "error:"),
            "\x1b[1;31merror:\x1b[0m"
        );
        assert_eq!(
            wrap(true, Style::BOLD_YELLOW, "warning:"),
            "\x1b[1;33mwarning:\x1b[0m"
        );
    }
}
