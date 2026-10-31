//! `vilan-rt-term` — the terminal mode behind `std::process::keys()` on the
//! native backend (tracker A173 S2; proposal/terminal.md §4.2 and §3.3).
//!
//! # What it does
//!
//! [`enter`] is `__term_enter`: when stdin is a terminal it saves the
//! terminal's settings, puts it in RAW mode (no echo, no line buffering, no
//! signal keys — Ctrl-C is a key), turns bracketed paste on when stdout is a
//! terminal too, and arranges for every road out of the process to put it all
//! back; [`leave`] is `__term_leave`, the `Keys`'s `Drop`. On a pipe it touches
//! nothing (terminal.md §4.3: there is no terminal to tell).
//!
//! The KEYS are not here. The bytes come from `vilan-rt`'s shared stdin pump
//! (`vilan_rt::process::stdin_bytes`) and std's own `KeyDecoder`, written in
//! vilan, decodes them on both backends (terminal.md §4.1): this crate is the
//! mode switch and nothing else, the twin of node's `setRawMode` plus the two
//! escape writes the JS helper makes.
//!
//! # The restore, road by road (§3.3)
//!
//! - the `Keys`'s last use, a `jump break`, a panic that unwinds, a cancelled
//!   task: the `Drop` calls [`leave`];
//! - `process::exit`, and a `main` that ends with the handle still in an
//!   unfinished task: `vilan_rt::process::at_exit`, registered by the first
//!   [`enter`] (node's twin is the helper's `'exit'` listener, beside node's
//!   own `ResetStdio`);
//! - SIGTERM, SIGHUP and SIGINT (unix): a handler restores the terminal from
//!   the saved settings — `tcsetattr` and `write` are async-signal-safe — then
//!   puts back whatever disposition was there before and re-raises, so the
//!   process still dies of the signal (128+n) as it would have, or a server's
//!   `vilan-rt-signal` handler still runs its graceful stop. The handlers are
//!   installed only while raw mode is on;
//! - SIGKILL: nothing can. The shell's `reset` is the answer, and the docs say
//!   so.
//!
//! [`leave`] is idempotent, so any two of those meeting is harmless.
//!
//! # Windows
//!
//! The console's input mode loses `ENABLE_LINE_INPUT`, `ENABLE_ECHO_INPUT` and
//! `ENABLE_PROCESSED_INPUT` and gains `ENABLE_VIRTUAL_TERMINAL_INPUT`, so the
//! console delivers its keys as the same VT byte sequences a unix terminal
//! sends and the same std decoder reads them (Windows 10 1607 and later); the
//! output handle gains `ENABLE_VIRTUAL_TERMINAL_PROCESSING` for redraws. No
//! signal handler: the console's close events end the process with its
//! console.

#![deny(unsafe_op_in_unsafe_fn)]

use std::io::{IsTerminal, Write};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

/// A `Keys` is live — on a terminal or on a pipe. One at a time.
static LIVE: AtomicBool = AtomicBool::new(false);
/// The live `Keys` put the terminal in raw mode.
static RAW: AtomicBool = AtomicBool::new(false);
/// The live `Keys` turned bracketed paste on (stdout is a terminal).
static PASTE: AtomicBool = AtomicBool::new(false);
/// The exit hook is registered (once per process).
static HOOKED: Once = Once::new();

const PASTE_ON: &[u8] = b"\x1b[?2004h";
const PASTE_OFF: &[u8] = b"\x1b[?2004l";

/// `__term_enter()`: `1` when stdin is a terminal and raw mode (and, with a
/// terminal stdout, bracketed paste) is now on; `0` on a pipe, nothing
/// touched; `-1` when a `Keys` is already live, nothing touched — `keys()`
/// panics with the sentence.
pub fn enter() -> i32 {
    if LIVE.swap(true, Ordering::SeqCst) {
        return -1;
    }
    // A stdin the terminal calls refuse (no terminal after all) reads as the
    // pipe it then is: decoded with no timeout, no mode to restore.
    if !std::io::stdin().is_terminal() || !platform::enter() {
        return 0;
    }
    RAW.store(true, Ordering::SeqCst);
    HOOKED.call_once(|| vilan_rt::process::at_exit(leave));
    if std::io::stdout().is_terminal() {
        PASTE.store(true, Ordering::SeqCst);
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(PASTE_ON);
        let _ = stdout.flush();
    }
    platform::install_signal_restore();
    1
}

/// `__term_leave()`: restores what [`enter`] changed, if anything. Idempotent.
pub fn leave() {
    if !LIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    if !RAW.swap(false, Ordering::SeqCst) {
        return;
    }
    platform::remove_signal_restore();
    // The mode first, then the paste switch: once the switch's bytes are out,
    // the terminal is already cooked (the pty pins read it in that order).
    platform::restore();
    if PASTE.swap(false, Ordering::SeqCst) {
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(PASTE_OFF);
        let _ = stdout.flush();
    }
}

#[cfg(unix)]
mod platform {
    use std::cell::UnsafeCell;
    use std::mem::MaybeUninit;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::{PASTE, PASTE_OFF};

    /// The terminal's settings from before raw mode — read by [`restore`] and
    /// by the signal handler, which may take no lock.
    struct Saved(UnsafeCell<MaybeUninit<libc::termios>>);

    // SAFETY: written only by `enter`, BEFORE `SAVED_VALID` is set and before
    // any handler that reads it is installed; read only while `SAVED_VALID` is
    // set. One `Keys` at a time (`LIVE`) means no second writer overlaps.
    unsafe impl Sync for Saved {}

    static SAVED: Saved = Saved(UnsafeCell::new(MaybeUninit::uninit()));
    static SAVED_VALID: AtomicBool = AtomicBool::new(false);

    /// The signals whose default ends the process with the terminal raw.
    const SIGNALS: [libc::c_int; 3] = [libc::SIGTERM, libc::SIGHUP, libc::SIGINT];

    /// The dispositions the handlers replaced, put back on [`leave`] and
    /// before a re-raise.
    struct Previous(UnsafeCell<[MaybeUninit<libc::sigaction>; 3]>);

    // SAFETY: written by `install_signal_restore` before `INSTALLED` is set;
    // read by `remove_signal_restore` and the handler only while it is.
    unsafe impl Sync for Previous {}

    static PREVIOUS: Previous = Previous(UnsafeCell::new([const { MaybeUninit::uninit() }; 3]));
    static INSTALLED: AtomicBool = AtomicBool::new(false);

    /// Saves the settings and enters raw mode. `false` if the terminal calls
    /// refuse, with nothing changed.
    pub fn enter() -> bool {
        let mut original = MaybeUninit::<libc::termios>::uninit();
        // SAFETY: `tcgetattr` writes a whole `termios` through the pointer
        // when it answers 0, which is the only case that reads it.
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, original.as_mut_ptr()) } != 0 {
            return false;
        }
        // SAFETY: initialized by the successful `tcgetattr` above.
        let original = unsafe { original.assume_init() };
        let mut raw = original;
        // SAFETY: `cfmakeraw` rewrites the flags of a valid `termios`.
        unsafe { libc::cfmakeraw(&raw mut raw) };
        // Output post-processing back on, so `\n` still returns the carriage
        // for `print`: libuv's raw mode does the same, and both backends print
        // alike under `keys()`.
        raw.c_oflag |= libc::OPOST | libc::ONLCR;
        // SAFETY: no reader exists yet — `SAVED_VALID` is clear and no handler
        // is installed (see `Saved`).
        unsafe { (*SAVED.0.get()).write(original) };
        SAVED_VALID.store(true, Ordering::SeqCst);
        // SAFETY: a valid `termios` for the process's own stdin.
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSADRAIN, &raw const raw) } != 0 {
            SAVED_VALID.store(false, Ordering::SeqCst);
            return false;
        }
        true
    }

    /// Puts the saved settings back, once.
    pub fn restore() {
        if !SAVED_VALID.swap(false, Ordering::SeqCst) {
            return;
        }
        // SAFETY: `SAVED_VALID` was set, so `SAVED` holds the settings
        // `enter` read; `tcsetattr` is async-signal-safe, which is what lets
        // the handler call this too.
        unsafe {
            libc::tcsetattr(
                libc::STDIN_FILENO,
                libc::TCSADRAIN,
                (*SAVED.0.get()).as_ptr(),
            );
        }
    }

    pub fn install_signal_restore() {
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return;
        }
        for (index, signal) in SIGNALS.into_iter().enumerate() {
            // SAFETY: a zeroed `sigaction` is a valid empty one; the handler
            // is an `extern "C" fn(c_int)`, the shape `sa_sigaction` takes
            // without `SA_SIGINFO`; the previous action is written into its
            // own slot, which nothing reads until `INSTALLED` is observed set.
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as usize;
                libc::sigemptyset(&raw mut action.sa_mask);
                let previous = (*PREVIOUS.0.get())[index].as_mut_ptr();
                libc::sigaction(signal, &raw const action, previous);
            }
        }
    }

    pub fn remove_signal_restore() {
        if !INSTALLED.swap(false, Ordering::SeqCst) {
            return;
        }
        for (index, signal) in SIGNALS.into_iter().enumerate() {
            // SAFETY: written by `install_signal_restore` before `INSTALLED`.
            unsafe {
                libc::sigaction(
                    signal,
                    (*PREVIOUS.0.get())[index].as_ptr(),
                    std::ptr::null_mut(),
                );
            }
        }
    }

    /// The handler: the terminal back as it was, the paste switch off, the
    /// previous disposition back, and the signal raised again — delivered to
    /// that disposition once this returns (the signal is blocked meanwhile).
    /// Only async-signal-safe calls: `tcsetattr`, `write`, `sigaction`,
    /// `raise`, and atomics.
    extern "C" fn on_signal(signal: libc::c_int) {
        restore();
        if PASTE.swap(false, Ordering::SeqCst) {
            // SAFETY: `write` of a static buffer to the process's own stdout.
            unsafe {
                libc::write(
                    libc::STDOUT_FILENO,
                    PASTE_OFF.as_ptr().cast(),
                    PASTE_OFF.len(),
                );
            }
        }
        if SIGNALS.contains(&signal) && INSTALLED.swap(false, Ordering::SeqCst) {
            for (other, each) in SIGNALS.into_iter().enumerate() {
                // SAFETY: written by `install_signal_restore` before
                // `INSTALLED`, which this handler just observed set.
                unsafe {
                    libc::sigaction(
                        each,
                        (*PREVIOUS.0.get())[other].as_ptr(),
                        std::ptr::null_mut(),
                    );
                }
            }
        }
        // SAFETY: `raise` is async-signal-safe.
        unsafe {
            libc::raise(signal);
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::sync::Mutex;

    use windows_sys::Win32::System::Console::{
        ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
        ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode,
        GetStdHandle, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleMode,
    };

    /// The input and output console modes from before (the output's only
    /// when stdout is a console).
    static SAVED: Mutex<Option<(u32, Option<u32>)>> = Mutex::new(None);

    pub fn enter() -> bool {
        // SAFETY: plain WinAPI FFI — each handle is null-checked before use,
        // and `GetConsoleMode` writes through a pointer to a live local.
        unsafe {
            let input = GetStdHandle(STD_INPUT_HANDLE);
            if input.is_null() {
                return false;
            }
            let mut input_mode = 0u32;
            if GetConsoleMode(input, &raw mut input_mode) == 0 {
                return false;
            }
            let raw = (input_mode
                & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT))
                | ENABLE_VIRTUAL_TERMINAL_INPUT;
            if SetConsoleMode(input, raw) == 0 {
                return false;
            }
            let output = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut output_mode = 0u32;
            let output_saved =
                if !output.is_null() && GetConsoleMode(output, &raw mut output_mode) != 0 {
                    SetConsoleMode(output, output_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                    Some(output_mode)
                } else {
                    None
                };
            *SAVED
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some((input_mode, output_saved));
        }
        true
    }

    pub fn restore() {
        let saved = SAVED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let Some((input_mode, output_mode)) = saved else {
            return;
        };
        // SAFETY: plain WinAPI FFI on the process's own standard handles.
        unsafe {
            let input = GetStdHandle(STD_INPUT_HANDLE);
            if !input.is_null() {
                SetConsoleMode(input, input_mode);
            }
            if let Some(output_mode) = output_mode {
                let output = GetStdHandle(STD_OUTPUT_HANDLE);
                if !output.is_null() {
                    SetConsoleMode(output, output_mode);
                }
            }
        }
    }

    pub fn install_signal_restore() {}

    pub fn remove_signal_restore() {}
}

/// Every other host: no terminal mode to enter, so `keys()` reads as a pipe.
#[cfg(not(any(unix, windows)))]
mod platform {
    pub fn enter() -> bool {
        false
    }

    pub fn restore() {}

    pub fn install_signal_restore() {}

    pub fn remove_signal_restore() {}
}
