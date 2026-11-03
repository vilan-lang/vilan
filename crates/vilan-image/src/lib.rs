//! `vilan-image` — `std::image`'s codec core (tracker M185; proposal/image.md).
//!
//! Decode PNG, JPEG, WebP and GIF (its first frame) into RGBA pixels, scale and
//! crop them, and encode PNG, JPEG and WebP — over the `image` crate, with no
//! vilan types anywhere in this crate.
//!
//! # One implementation, two hosts
//!
//! This crate is what BOTH backends run: natively through `vilan-rt-image`
//! (the externs' Rust renderings over `vilan-rt`'s `Bytes`), and on node as the
//! same source compiled for wasm32 (`vilan-image-wasm`, whose committed blob
//! the JS emitter carries). Keeping the two backends' output close is settled
//! by not having two implementations — and every claim about codec behaviour
//! is a unit test here, on the host.
//!
//! # Why the resampler is written here
//!
//! `image::imageops::resize` would do Lanczos3, but its kernel calls `f32::sin`,
//! which is the HOST's libm natively (glibc, the macOS or Windows CRT) and
//! compiler-builtins' port of musl on wasm32 — sines that may disagree in the
//! last bit, and a last-bit disagreement in a weight is now and then a pixel
//! one level apart between a native server and a node one. [`sin_pi`] is
//! written from additions and multiplications alone, which IEEE 754 rounds the
//! same way on every target (Rust does not contract `a * b + c` into a fused
//! multiply-add), so the weights — and the pixels — are the same bits on both
//! backends. It also resamples non-opaque pictures with premultiplied alpha,
//! which `imageops::resize` assumes of its input and does not do itself (a
//! straight-alpha resize bleeds the colour of transparent pixels into the
//! edge of a logo).
//!
//! # The seam's integer codes
//!
//! `Format` and `Fit` cross the extern seam as small integers, ONE table for
//! both hosts — `std/src/image.vl` carries its copy beside its enums:
//!
//! | code | `Format` | `Fit`     |
//! |------|----------|-----------|
//! | 0    | `Png`    | `Contain` |
//! | 1    | `Jpeg`   | `Cover`   |
//! | 2    | `Webp`   | `Fill`    |
//! | 3    | `Gif`    |           |
//!
//! `sniff` answers -1 for bytes that are none of the four.

#![forbid(unsafe_code)]

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{
    DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Limits,
    RgbaImage,
};

/// `std::image::decode`'s default pixel limit (proposal/image.md §6 Q5): a
/// 12-megapixel phone photo decodes, a 60,000 x 60,000 "picture" does not.
pub const DEFAULT_MAX_PIXELS: u64 = 50_000_000;

/// The largest picture [`Image::resized`] answers: `decode`'s default limit.
/// The box is the PROGRAM's choice rather than the upload's, so exceeding it is
/// a refusal the program cannot recover from in any useful way — the vilan
/// surface turns it into a panic naming both sizes — and it keeps a box like
/// 100,000 x 100,000 from asking for 40 GB (a trap on wasm32, an abort
/// natively).
pub const MAX_RESIZED_PIXELS: u64 = DEFAULT_MAX_PIXELS;

/// The four formats `decode` reads. `encode` writes the first three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Webp,
    Gif,
}

impl Format {
    /// The seam's integer for this format (the table in the crate header).
    pub fn code(self) -> i32 {
        match self {
            Format::Png => 0,
            Format::Jpeg => 1,
            Format::Webp => 2,
            Format::Gif => 3,
        }
    }

    /// The format a seam integer names; `None` for any other integer.
    pub fn from_code(code: i32) -> Option<Format> {
        match code {
            0 => Some(Format::Png),
            1 => Some(Format::Jpeg),
            2 => Some(Format::Webp),
            3 => Some(Format::Gif),
            _ => None,
        }
    }

    /// The format's name as an error sentence writes it.
    pub fn name(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Webp => "WebP",
            Format::Gif => "GIF",
        }
    }

    fn image_format(self) -> ImageFormat {
        match self {
            Format::Png => ImageFormat::Png,
            Format::Jpeg => ImageFormat::Jpeg,
            Format::Webp => ImageFormat::WebP,
            Format::Gif => ImageFormat::Gif,
        }
    }
}

/// How [`Image::resized`] maps a picture onto a `width` x `height` box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Keep the aspect, inside the box: the result may be smaller on one side.
    Contain,
    /// Keep the aspect, filling the box: the overflow is cropped, centred.
    Cover,
    /// Stretch to exactly the box.
    Fill,
}

impl Fit {
    /// The seam's integer for this fit (the table in the crate header).
    pub fn code(self) -> i32 {
        match self {
            Fit::Contain => 0,
            Fit::Cover => 1,
            Fit::Fill => 2,
        }
    }

    /// The fit a seam integer names; `None` for any other integer.
    pub fn from_code(code: i32) -> Option<Fit> {
        match code {
            0 => Some(Fit::Contain),
            1 => Some(Fit::Cover),
            2 => Some(Fit::Fill),
            _ => None,
        }
    }
}

/// The format `bytes` would decode as, from their first bytes alone: PNG's
/// `\x89PNG`, JPEG's `\xFF\xD8`, WebP's `RIFF....WEBP`, GIF's `GIF8`.
pub fn sniff(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(b"\x89PNG") {
        Some(Format::Png)
    } else if bytes.starts_with(b"\xFF\xD8") {
        Some(Format::Jpeg)
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some(Format::Webp)
    } else if bytes.starts_with(b"GIF8") {
        Some(Format::Gif)
    } else {
        None
    }
}

/// A decoded picture: `width` x `height` pixels, RGBA, 8 bits a channel,
/// row-major. Immutable: every operation answers a new picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    buffer: RgbaImage,
}

impl Image {
    /// Decode `bytes` — the format sniffed ([`sniff`]), the picture's own
    /// orientation applied (a phone photo's EXIF orientation 1-8), so
    /// [`Image::width`] and [`Image::height`] are the UPRIGHT size.
    ///
    /// A picture whose header claims more than `max_pixels` pixels is refused
    /// from the header, BEFORE the pixels are allocated or decoded.
    pub fn decode(bytes: &[u8], max_pixels: u64) -> Result<Image, String> {
        let format = sniff(bytes).ok_or_else(|| "not a PNG, JPEG, WebP or GIF".to_string())?;
        let failed = |error: image::ImageError| {
            format!("the {} could not be decoded: {error}", format.name())
        };
        let mut reader = ImageReader::with_format(Cursor::new(bytes), format.image_format());
        // The pixel limit is the check that matters and it is made below, from
        // the header. `image`'s own allocation ceiling (512 MiB by default) is
        // raised to cover a limit set past it, so a caller who asked for a
        // larger picture gets one rather than a second, unrelated refusal.
        let mut limits = Limits::default();
        let default_alloc = limits.max_alloc.unwrap_or(0);
        limits.max_alloc = Some(default_alloc.max(max_pixels.saturating_mul(8)));
        reader.limits(limits);
        let mut decoder = reader.into_decoder().map_err(failed)?;
        let (width, height) = decoder.dimensions();
        let pixels = u64::from(width) * u64::from(height);
        if pixels > max_pixels {
            return Err(format!(
                "{width} x {height} exceeds the {}-pixel limit",
                grouped(max_pixels)
            ));
        }
        let orientation = decoder.orientation().map_err(failed)?;
        let mut decoded = DynamicImage::from_decoder(decoder).map_err(failed)?;
        decoded.apply_orientation(orientation);
        Ok(Image {
            buffer: decoded.into_rgba8(),
        })
    }

    /// A picture from raw RGBA pixels, `width * height * 4` bytes row-major;
    /// any other length is refused.
    pub fn from_rgba(width: u64, height: u64, pixels: Vec<u8>) -> Result<Image, String> {
        let expected = width
            .checked_mul(height)
            .and_then(|count| count.checked_mul(4));
        let refused = || {
            format!(
                "{} bytes are not {width} x {height} RGBA pixels ({} bytes)",
                pixels.len(),
                expected.map_or_else(|| "too many".to_string(), |bytes| bytes.to_string())
            )
        };
        if expected != Some(pixels.len() as u64) {
            return Err(refused());
        }
        let (Ok(narrow_width), Ok(narrow_height)) = (u32::try_from(width), u32::try_from(height))
        else {
            return Err(refused());
        };
        let buffer = RgbaImage::from_raw(narrow_width, narrow_height, pixels)
            .expect("the length was checked against the dimensions above");
        Ok(Image { buffer })
    }

    pub fn width(&self) -> u32 {
        self.buffer.width()
    }

    pub fn height(&self) -> u32 {
        self.buffer.height()
    }

    /// The pixels, RGBA row-major.
    pub fn pixels(&self) -> &[u8] {
        self.buffer.as_raw()
    }

    /// The pixels, taken.
    pub fn into_pixels(self) -> Vec<u8> {
        self.buffer.into_raw()
    }

    /// Scaled to `width` x `height` by `fit` (Lanczos3; upscaling allowed).
    ///
    /// `Contain` keeps the aspect inside the box, rounding each side to the
    /// nearest pixel and never below one; `Cover` crops the source to the box's
    /// aspect, centred, and scales that window to exactly the box; `Fill`
    /// stretches. A zero side of the box is taken as one pixel. An EMPTY picture
    /// (a zero side of its own) has nothing to sample and is answered as it is.
    ///
    /// A RESULT larger than [`MAX_RESIZED_PIXELS`] is refused — `a 100000 x
    /// 100000 result exceeds the 50,000,000-pixel limit` — before anything is
    /// allocated. It is the result that is measured, so a `Contain` into a
    /// huge box that comes out small is fine.
    pub fn resized(&self, width: u64, height: u64, fit: Fit) -> Result<Image, String> {
        let (source_width, source_height) = (self.width(), self.height());
        if source_width == 0 || source_height == 0 {
            return Ok(self.clone());
        }
        let box_width = width.max(1);
        let box_height = height.max(1);
        let (result_width, result_height) = match fit {
            Fit::Fill | Fit::Cover => (box_width, box_height),
            Fit::Contain => contain_size(source_width, source_height, box_width, box_height),
        };
        if u128::from(result_width) * u128::from(result_height) > u128::from(MAX_RESIZED_PIXELS) {
            return Err(format!(
                "a {result_width} x {result_height} result exceeds the {}-pixel limit",
                grouped(MAX_RESIZED_PIXELS)
            ));
        }
        // Within the limit, every side fits a `u32`.
        let (result_width, result_height) = (result_width as u32, result_height as u32);
        Ok(match fit {
            Fit::Fill | Fit::Contain => Image {
                buffer: resample(&self.buffer, result_width, result_height),
            },
            Fit::Cover => {
                let (x, y, window_width, window_height) =
                    cover_window(source_width, source_height, result_width, result_height);
                let window = self.cropped(
                    u64::from(x),
                    u64::from(y),
                    u64::from(window_width),
                    u64::from(window_height),
                );
                Image {
                    buffer: resample(&window.buffer, result_width, result_height),
                }
            }
        })
    }

    /// The `width` x `height` window at (`x`, `y`), clamped to the picture:
    /// a window reaching past an edge stops at it, and one starting past it is
    /// empty.
    pub fn cropped(&self, x: u64, y: u64, width: u64, height: u64) -> Image {
        let (own_width, own_height) = (u64::from(self.width()), u64::from(self.height()));
        let left = x.min(own_width);
        let top = y.min(own_height);
        let window_width = width.min(own_width - left);
        let window_height = height.min(own_height - top);
        let mut pixels = Vec::with_capacity((window_width * window_height * 4) as usize);
        let stride = own_width as usize * 4;
        for row in top..top + window_height {
            let start = row as usize * stride + left as usize * 4;
            pixels.extend_from_slice(&self.pixels()[start..start + window_width as usize * 4]);
        }
        Image {
            buffer: RgbaImage::from_raw(window_width as u32, window_height as u32, pixels)
                .expect("the window lies inside the picture"),
        }
    }

    /// Encode as `format`. `quality` (1-100, clamped) is JPEG's; PNG and WebP
    /// are lossless and ignore it (proposal/image.md §6 Q2 — `image`'s pure-Rust
    /// WebP encoder is lossless only). JPEG has no alpha channel, so a
    /// transparent pixel is flattened onto BLACK — what a canvas, sharp,
    /// libvips and the `image` crate itself do (M190, the owner's ruling over
    /// §6 Q4's white). GIF is decode-only.
    pub fn encode(&self, format: Format, quality: i32) -> Result<Vec<u8>, String> {
        let (width, height) = (self.width(), self.height());
        let mut out = Vec::new();
        let failed = |error: image::ImageError| {
            format!("the {} could not be encoded: {error}", format.name())
        };
        match format {
            Format::Png => PngEncoder::new(&mut out)
                .write_image(self.pixels(), width, height, ExtendedColorType::Rgba8)
                .map_err(failed)?,
            Format::Jpeg => {
                let quality = quality.clamp(1, 100) as u8;
                JpegEncoder::new_with_quality(&mut out, quality)
                    .write_image(
                        &flattened_on_black(self.pixels()),
                        width,
                        height,
                        ExtendedColorType::Rgb8,
                    )
                    .map_err(failed)?
            }
            Format::Webp => WebPEncoder::new_lossless(&mut out)
                .write_image(self.pixels(), width, height, ExtendedColorType::Rgba8)
                .map_err(failed)?,
            Format::Gif => return Err("GIF cannot be encoded".to_string()),
        }
        Ok(out)
    }
}

/// `n` with its thousands grouped by commas — `1,000,000` — as the limit's
/// error sentence writes it.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// `Contain`'s size: the source scaled by the smaller of the two box ratios,
/// the bound side exactly the box's and the other rounded to the nearest
/// pixel (half away from zero), never below one. Integer arithmetic, so the
/// answer is exact.
fn contain_size(
    source_width: u32,
    source_height: u32,
    box_width: u64,
    box_height: u64,
) -> (u64, u64) {
    // u128: a box side is any `u64` the program asked for, and the products
    // below must not wrap before the limit gets to refuse them.
    let (sw, sh) = (u128::from(source_width), u128::from(source_height));
    let (bw, bh) = (u128::from(box_width), u128::from(box_height));
    // The width binds when bw / sw <= bh / sh.
    if bw * sh <= bh * sw {
        let height = (2 * sh * bw + sw) / (2 * sw);
        (box_width, height.clamp(1, bh) as u64)
    } else {
        let width = (2 * sw * bh + sh) / (2 * sh);
        (width.clamp(1, bw) as u64, box_height)
    }
}

/// `Cover`'s source window: the largest window of the box's aspect that fits
/// the source, centred (an odd leftover puts the extra pixel after the
/// window), as `(x, y, width, height)`.
fn cover_window(
    source_width: u32,
    source_height: u32,
    box_width: u32,
    box_height: u32,
) -> (u32, u32, u32, u32) {
    let (sw, sh) = (u64::from(source_width), u64::from(source_height));
    let (bw, bh) = (u64::from(box_width), u64::from(box_height));
    if sw * bh >= sh * bw {
        // The source is wider than the box: full height, a centred band of it.
        let width = ((2 * sh * bw + bh) / (2 * bh)).clamp(1, sw);
        (((sw - width) / 2) as u32, 0, width as u32, source_height)
    } else {
        let height = ((2 * sw * bh + bw) / (2 * bw)).clamp(1, sh);
        (0, ((sh - height) / 2) as u32, source_width, height as u32)
    }
}

/// JPEG's input: each RGBA pixel composited over black, `c * a` over 255,
/// rounded — the colour weighted by its coverage, a transparent pixel black.
fn flattened_on_black(rgba: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for pixel in rgba.as_chunks::<4>().0.iter() {
        let alpha = u32::from(pixel[3]);
        for channel in &pixel[..3] {
            let composited = (u32::from(*channel) * alpha + 127) / 255;
            rgb.push(composited as u8);
        }
    }
    rgb
}

// --- The resampler ----------------------------------------------------------

/// `sin(pi * x)`, from additions and multiplications only (see the crate
/// header for why): `x` is reduced to `n + r` with `|r| <= 1/2`, and
/// `sin(pi * r)` is its Taylor series through the 17th power, whose error at
/// `pi / 2` is under 1e-12 — far below anything a weight normalized and summed
/// into an 8-bit channel can show.
fn sin_pi(x: f64) -> f64 {
    let whole = x.round();
    let angle = std::f64::consts::PI * (x - whole);
    let square = angle * angle;
    // Horner over the odd powers: angle * (1 - a²/3! (1 - a²/(4·5) (1 - …))).
    let mut series = 1.0;
    for (low, high) in [
        (16.0, 17.0),
        (14.0, 15.0),
        (12.0, 13.0),
        (10.0, 11.0),
        (8.0, 9.0),
        (6.0, 7.0),
        (4.0, 5.0),
        (2.0, 3.0),
    ] {
        series = 1.0 - square / (low * high) * series;
    }
    let sine = angle * series;
    if whole.rem_euclid(2.0) == 0.0 {
        sine
    } else {
        -sine
    }
}

/// `sin(pi x) / (pi x)`, 1 at 0.
fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        1.0
    } else {
        sin_pi(x) / (std::f64::consts::PI * x)
    }
}

/// The Lanczos kernel of three lobes.
fn lanczos3(x: f64) -> f64 {
    if x.abs() >= 3.0 {
        0.0
    } else {
        sinc(x) * sinc(x / 3.0)
    }
}

/// One output position's taps along an axis: the first source index it reads
/// and the normalized weight of each source index from there.
struct Taps {
    first: usize,
    weights: Vec<f64>,
}

/// The taps of every output position along an axis of `source` pixels scaled
/// to `target`. Downscaling widens the kernel by the ratio (so every source
/// pixel contributes — the filter is a low-pass, not a point sample);
/// upscaling keeps it at three lobes. Pixel CENTRES are mapped, so a picture
/// scaled to its own size reads each pixel with weight one.
fn taps(source: u32, target: u32) -> Vec<Taps> {
    let ratio = f64::from(source) / f64::from(target);
    let scale = ratio.max(1.0);
    let support = 3.0 * scale;
    (0..target)
        .map(|position| {
            let centre = (f64::from(position) + 0.5) * ratio;
            let first = (centre - support).floor().max(0.0) as usize;
            let last = ((centre + support).ceil() as usize).min(source as usize);
            let mut weights: Vec<f64> = (first..last)
                .map(|index| lanczos3((index as f64 + 0.5 - centre) / scale))
                .collect();
            let total: f64 = weights.iter().sum();
            for weight in &mut weights {
                *weight /= total;
            }
            Taps { first, weights }
        })
        .collect()
}

/// An 8-bit channel from a resampled value: rounded half away from zero and
/// clamped (Lanczos' negative lobes overshoot at a hard edge).
fn channel(value: f64) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

/// `image` scaled to `width` x `height`, Lanczos3, separably: rows first into
/// an intermediate of `width` x source height, then columns.
///
/// A picture with any pixel short of opaque is resampled PREMULTIPLIED — each
/// colour weighted by its alpha going in and divided by the resampled alpha
/// coming out — so a fully transparent pixel's colour cannot bleed into its
/// neighbours. An opaque picture skips both steps; the result is the same.
fn resample(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    let (source_width, source_height) = image.dimensions();
    if (source_width, source_height) == (width, height) {
        return image.clone();
    }
    let source = image.as_raw();
    let premultiply = source
        .as_chunks::<4>()
        .0
        .iter()
        .any(|pixel| pixel[3] != 255);
    let horizontal = taps(source_width, width);
    let vertical = taps(source_height, height);

    // Rows: source_height rows of `width` pixels, as f32 RGBA (premultiplied
    // when `premultiply`).
    let mut rows = vec![0f32; width as usize * source_height as usize * 4];
    let source_stride = source_width as usize * 4;
    for row in 0..source_height as usize {
        let line = &source[row * source_stride..(row + 1) * source_stride];
        for (column, tap) in horizontal.iter().enumerate() {
            let mut sum = [0f64; 4];
            for (offset, weight) in tap.weights.iter().enumerate() {
                let pixel = &line[(tap.first + offset) * 4..(tap.first + offset) * 4 + 4];
                let alpha = f64::from(pixel[3]);
                let factor = if premultiply { alpha / 255.0 } else { 1.0 };
                sum[0] += weight * f64::from(pixel[0]) * factor;
                sum[1] += weight * f64::from(pixel[1]) * factor;
                sum[2] += weight * f64::from(pixel[2]) * factor;
                sum[3] += weight * alpha;
            }
            let at = (row * width as usize + column) * 4;
            for (slot, value) in sum.iter().enumerate() {
                rows[at + slot] = *value as f32;
            }
        }
    }

    // Columns: each output row sums the intermediate rows its taps name.
    let mut out = vec![0u8; width as usize * height as usize * 4];
    let row_stride = width as usize * 4;
    for (output_row, tap) in vertical.iter().enumerate() {
        for column in 0..width as usize {
            let mut sum = [0f64; 4];
            for (offset, weight) in tap.weights.iter().enumerate() {
                let at = (tap.first + offset) * row_stride + column * 4;
                for (slot, total) in sum.iter_mut().enumerate() {
                    *total += weight * f64::from(rows[at + slot]);
                }
            }
            let at = (output_row * width as usize + column) * 4;
            let alpha = channel(sum[3]);
            out[at + 3] = alpha;
            for slot in 0..3 {
                out[at + slot] = if !premultiply {
                    channel(sum[slot])
                } else if alpha == 0 {
                    0
                } else {
                    channel(sum[slot] * 255.0 / sum[3].clamp(0.0, 255.0))
                };
            }
        }
    }
    RgbaImage::from_raw(width, height, out).expect("the buffer holds width x height pixels")
}

#[cfg(test)]
mod tests;
