//! `vilan-image-wasm` — the export layer that makes `vilan-image` a wasm32
//! module node can instantiate (tracker M185; proposal/image.md §2.2).
//!
//! # The protocol
//!
//! ONE scratch picture lives in the module (the "slot"), and one output buffer
//! (the encoded bytes, or a refusal's sentence). The JS helpers
//! (`transformer.rs`'s `__image_*`) drive it a call at a time:
//!
//! 1. `image_alloc(len)` a buffer, write the input bytes into it;
//! 2. `image_decode(pointer, len, max_pixels)` or `image_load(width, height,
//!    pointer, len)` — each CONSUMES the buffer — fills the slot, answering 0,
//!    or 1 with the sentence in the output;
//! 3. `image_resize` (0, or 1 with a refusal) and `image_crop` replace the
//!    slot; `image_encode` fills the output;
//! 4. `image_width`/`image_height`/`image_pixels`/`image_pixels_len` and
//!    `image_output`/`image_output_len` are read, and the JS side COPIES the
//!    bytes out into a `Uint8Array` of its own.
//!
//! Nothing in wasm memory outlives the helper call that made it — the JS
//! `Image` is a plain `{ width, height, pixels }` holding its own copy — so
//! there are no handles to leak and no finalizer to write.
//!
//! # Counts are `f64`
//!
//! A vilan `usize` is a JS number, which reaches a wasm `i32` parameter
//! through `ToInt32` — modulo 2^32. Every count that crosses here (a pixel
//! limit, a box side, a crop offset) is taken as an `f64` instead and
//! converted with Rust's saturating `as`, so a large count means what it says
//! (a box side past `u32::MAX` clamps, as it does natively) rather than what
//! its low 32 bits say.
//!
//! # `unsafe`
//!
//! The one place it is: reading the bytes the host wrote at a pointer it was
//! handed, and taking back a buffer `image_alloc` leaked to it. Everything
//! between is `vilan-image`, which forbids `unsafe`.

use std::cell::RefCell;

use vilan_image::{Fit, Format, Image};

thread_local! {
    /// The scratch picture.
    static SLOT: RefCell<Option<Image>> = const { RefCell::new(None) };
    /// The last encode's bytes, or the last refusal's sentence (UTF-8).
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// 0 for success, 1 for a refusal whose sentence is in the output.
fn status(result: Result<Image, String>) -> i32 {
    match result {
        Ok(image) => {
            SLOT.with(|slot| *slot.borrow_mut() = Some(image));
            0
        }
        Err(sentence) => {
            OUTPUT.with(|output| *output.borrow_mut() = sentence.into_bytes());
            1
        }
    }
}

/// The slot's picture, for an operation that reads it. An empty slot is a
/// helper bug (every operation is preceded by a load), so it is a trap.
fn with_slot<T>(body: impl FnOnce(&Image) -> T) -> T {
    SLOT.with(|slot| {
        body(
            slot.borrow()
                .as_ref()
                .expect("an image operation ran with no picture loaded"),
        )
    })
}

fn replace_slot(image: Image) {
    SLOT.with(|slot| *slot.borrow_mut() = Some(image));
}

/// A count from the host: an `f64` holding a non-negative integer, read with
/// Rust's saturating conversion (NaN is 0).
fn count(value: f64) -> u64 {
    value as u64
}

/// Takes back a buffer `image_alloc` handed out.
///
/// # Safety
///
/// `pointer` and `len` must be exactly what one `image_alloc(len)` answered,
/// not yet freed or consumed.
unsafe fn take(pointer: *mut u8, len: usize) -> Vec<u8> {
    // SAFETY: the caller's contract — a boxed slice of exactly `len` bytes
    // that `image_alloc` leaked.
    let boxed = unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(pointer, len)) };
    boxed.into_vec()
}

/// A zeroed buffer of `len` bytes for the host to write into; it is consumed
/// by `image_decode`/`image_load`/`image_sniff` or released by `image_free`.
#[unsafe(no_mangle)]
pub extern "C" fn image_alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()).cast::<u8>()
}

/// Releases a buffer `image_alloc` handed out.
///
/// # Safety
///
/// As [`take`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_free(pointer: *mut u8, len: usize) {
    // SAFETY: forwarded from this function's own contract.
    drop(unsafe { take(pointer, len) });
}

/// The format the bytes in the buffer would decode as — its seam code, or -1.
/// The buffer is consumed.
///
/// # Safety
///
/// As [`take`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_sniff(pointer: *mut u8, len: usize) -> i32 {
    // SAFETY: forwarded from this function's own contract.
    let bytes = unsafe { take(pointer, len) };
    vilan_image::sniff(&bytes).map_or(-1, Format::code)
}

/// Decodes the buffer into the slot (consuming it): 0, or 1 with the refusal.
///
/// # Safety
///
/// As [`take`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_decode(pointer: *mut u8, len: usize, max_pixels: f64) -> i32 {
    // SAFETY: forwarded from this function's own contract.
    let bytes = unsafe { take(pointer, len) };
    status(Image::decode(&bytes, count(max_pixels)))
}

/// Loads raw RGBA pixels into the slot (consuming the buffer): 0, or 1 with
/// the refusal (a length that is not `width * height * 4`).
///
/// # Safety
///
/// As [`take`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn image_load(width: f64, height: f64, pointer: *mut u8, len: usize) -> i32 {
    // SAFETY: forwarded from this function's own contract.
    let pixels = unsafe { take(pointer, len) };
    status(Image::from_rgba(count(width), count(height), pixels))
}

#[unsafe(no_mangle)]
pub extern "C" fn image_width() -> u32 {
    with_slot(Image::width)
}

#[unsafe(no_mangle)]
pub extern "C" fn image_height() -> u32 {
    with_slot(Image::height)
}

/// Where the slot's pixels start — valid until the next call that changes the
/// slot.
#[unsafe(no_mangle)]
pub extern "C" fn image_pixels() -> *const u8 {
    with_slot(|image| image.pixels().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn image_pixels_len() -> usize {
    with_slot(|image| image.pixels().len())
}

/// Replaces the slot with itself scaled (`fit` is the seam's `Fit` code): 0,
/// or 1 with the refusal (a result over the pixel limit) and the slot as it
/// was.
#[unsafe(no_mangle)]
pub extern "C" fn image_resize(width: f64, height: f64, fit: i32) -> i32 {
    let fit = Fit::from_code(fit).expect("the helper passes a `Fit` code");
    status(with_slot(|image| {
        image.resized(count(width), count(height), fit)
    }))
}

/// Replaces the slot with a window of itself, clamped.
#[unsafe(no_mangle)]
pub extern "C" fn image_crop(x: f64, y: f64, width: f64, height: f64) {
    let cropped = with_slot(|image| image.cropped(count(x), count(y), count(width), count(height)));
    replace_slot(cropped);
}

/// Encodes the slot into the output: 0 with the bytes, or 1 with the refusal.
#[unsafe(no_mangle)]
pub extern "C" fn image_encode(format: i32, quality: i32) -> i32 {
    let format = Format::from_code(format).expect("the helper passes a `Format` code");
    let (code, bytes) = match with_slot(|image| image.encode(format, quality)) {
        Ok(bytes) => (0, bytes),
        Err(sentence) => (1, sentence.into_bytes()),
    };
    OUTPUT.with(|output| *output.borrow_mut() = bytes);
    code
}

/// Where the output starts — valid until the next call that writes it.
#[unsafe(no_mangle)]
pub extern "C" fn image_output() -> *const u8 {
    OUTPUT.with(|output| output.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn image_output_len() -> usize {
    OUTPUT.with(|output| output.borrow().len())
}
