//! `vilan-rt-image` — `std::image` on the native backend (tracker M185;
//! proposal/image.md).
//!
//! # What this is a twin OF
//!
//! `vilan/std/src/image.vl` binds nine `__image_*` host helpers, which on the
//! JS backend are `transformer.rs`'s helpers over the wasm build of
//! `vilan-image`. Each function below is the native rendering of one of them,
//! over the SAME core (`vilan-image`), so the two backends decode, scale and
//! encode with one implementation and print the same pixels.
//!
//! # The seam
//!
//! `Format` and `Fit` cross as small integers — the table is `vilan-image`'s
//! crate header, and `image.vl` keeps its copy beside the enums. `sniff`
//! answers -1 for "none of the four"; `image.vl` makes the `Option`. A refusal
//! the vilan surface types as a `Result` is an `Err(Str)` here, the sentence
//! the core wrote — the same sentence the wasm build hands the JS helper.
//!
//! # Values
//!
//! An [`Image`] is immutable — every operation answers a new one — so its
//! pixels are shared behind an `Rc` and a copy is a count bump, which is
//! indistinguishable from the deep copy rule 1 promises. `rgba` copies OUT
//! (the caller may write the `Bytes` it gets) and `from_rgba` copies IN (the
//! caller may write the `Bytes` it gave), exactly as the JS helpers do.

#![forbid(unsafe_code)]

use std::rc::Rc;

use vilan_image::{Fit, Format};
use vilan_rt::bytes::Bytes;
use vilan_rt::{Js, Json, Str, panic_with, str_new};

/// `std::image::Image` — a decoded picture, RGBA, 8 bits a channel.
#[derive(Clone, Debug)]
pub struct Image(Rc<vilan_image::Image>);

impl Image {
    fn new(image: vilan_image::Image) -> Image {
        Image(Rc::new(image))
    }

    /// `__image_width(image)`.
    pub fn width(&self) -> usize {
        self.0.width() as usize
    }

    /// `__image_height(image)`.
    pub fn height(&self) -> usize {
        self.0.height() as usize
    }

    /// `__image_rgba(image)` — the pixels, COPIED into a fresh `Bytes`.
    pub fn rgba(&self) -> Bytes {
        Bytes::from_vec(self.0.pixels().to_vec())
    }

    /// `__image_resize(image, width, height, fit)`.
    pub fn resized(&self, width: usize, height: usize, fit: i32) -> Image {
        let fit = Fit::from_code(fit)
            .unwrap_or_else(|| panic_with(&format!("{fit} is not a `std::image::Fit` code")));
        Image::new(self.0.resized(width as u64, height as u64, fit))
    }

    /// `__image_crop(image, x, y, width, height)`.
    pub fn cropped(&self, x: usize, y: usize, width: usize, height: usize) -> Image {
        Image::new(
            self.0
                .cropped(x as u64, y as u64, width as u64, height as u64),
        )
    }

    /// `__image_encode(image, format, quality)`.
    pub fn encode(&self, format: i32, quality: i32) -> Result<Bytes, Str> {
        let format = Format::from_code(format)
            .unwrap_or_else(|| panic_with(&format!("{format} is not a `std::image::Format` code")));
        self.0
            .encode(format, quality)
            .map(Bytes::from_vec)
            .map_err(|error| str_new(&error))
    }
}

/// `__image_decode(bytes, max_pixels)`.
pub fn decode(bytes: &Bytes, max_pixels: usize) -> Result<Image, Str> {
    vilan_image::Image::decode(&bytes.as_slice(), max_pixels as u64)
        .map(Image::new)
        .map_err(|error| str_new(&error))
}

/// `__image_sniff(bytes)` — the format's seam code, or -1.
pub fn sniff(bytes: &Bytes) -> i32 {
    vilan_image::sniff(&bytes.as_slice()).map_or(-1, Format::code)
}

/// `__image_from_rgba(width, height, pixels)` — the pixels COPIED in.
pub fn from_rgba(width: usize, height: usize, pixels: &Bytes) -> Result<Image, Str> {
    vilan_image::Image::from_rgba(width as u64, height as u64, pixels.to_vec())
        .map(Image::new)
        .map_err(|error| str_new(&error))
}

/// Identity, as the JS object's `===` is: two pictures are one when they are
/// the same value, not when their pixels agree (that is what `rgba()` and a
/// comparison of bytes are for, on both backends).
impl PartialEq for Image {
    fn eq(&self, other: &Image) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Js for Image {
    /// Node prints the JS `Image` as the plain object it is, its pixels as a
    /// `Uint8Array`'s own inspection — refused here, as printing a `Bytes` is.
    fn js(&self) -> String {
        panic_with(
            "printing an `Image` is a host object's own inspection, which the native backend \
             does not reproduce (print its `width()`, `height()` or `rgba()` bytes)",
        )
    }
}

/// `JSON.stringify` of the JS `Image` — `{ width, height, pixels }`, the
/// pixels a `Uint8Array` stringified as the object of indices it is.
impl Json for Image {
    fn json(&self) -> String {
        format!(
            "{{\"width\":{},\"height\":{},\"pixels\":{}}}",
            self.width(),
            self.height(),
            self.rgba().json()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_by_two() -> Image {
        let pixels: Vec<u8> = (0..16).map(|byte| byte * 16).collect();
        from_rgba(2, 2, &Bytes::from_vec(pixels)).expect("a whole picture")
    }

    #[test]
    fn rgba_and_from_rgba_copy_rather_than_share() {
        let given = Bytes::from_vec(vec![9; 4]);
        let image = from_rgba(1, 1, &given).unwrap();
        let out = image.rgba();
        assert_eq!(out.to_vec(), vec![9; 4]);
        assert!(out != image.rgba(), "each `rgba()` is a fresh buffer");
    }

    #[test]
    fn the_seam_round_trips_a_png() {
        let image = two_by_two();
        let encoded = image.encode(Format::Png.code(), 0).unwrap();
        assert_eq!(sniff(&encoded), Format::Png.code());
        let decoded = decode(&encoded, 4).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
        assert_eq!(decoded.rgba().to_vec(), image.rgba().to_vec());
        assert_eq!(
            decode(&encoded, 3).unwrap_err().as_ref(),
            "2 x 2 exceeds the 3-pixel limit"
        );
    }

    #[test]
    fn the_seam_codes_reach_the_core() {
        let image = two_by_two();
        assert_eq!(sniff(&Bytes::from_vec(b"text".to_vec())), -1);
        assert_eq!(
            image.encode(Format::Gif.code(), 80).unwrap_err().as_ref(),
            "GIF cannot be encoded"
        );
        let filled = image.resized(1, 1, Fit::Fill.code());
        assert_eq!((filled.width(), filled.height()), (1, 1));
        let cropped = image.cropped(1, 0, 5, 5);
        assert_eq!((cropped.width(), cropped.height()), (1, 2));
    }

    #[test]
    fn json_is_the_js_objects_stringification() {
        let image = from_rgba(1, 1, &Bytes::from_vec(vec![1, 2, 3, 4])).unwrap();
        assert_eq!(
            image.json(),
            "{\"width\":1,\"height\":1,\"pixels\":{\"0\":1,\"1\":2,\"2\":3,\"3\":4}}"
        );
    }
}
