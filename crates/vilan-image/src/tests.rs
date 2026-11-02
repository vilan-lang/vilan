//! The codec core's pins (proposal/image.md §4). Every fixture is SYNTHESIZED
//! here — encoded by the crate in the test — so no binary fixture is committed.

use super::*;

/// A `width` x `height` picture whose pixel at (x, y) is `paint(x, y)`.
fn painted(width: u32, height: u32, paint: impl Fn(u32, u32) -> [u8; 4]) -> Image {
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&paint(x, y));
        }
    }
    Image::from_rgba(u64::from(width), u64::from(height), pixels).expect("a whole picture")
}

fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
    let at = (y as usize * image.width() as usize + x as usize) * 4;
    image.pixels()[at..at + 4].try_into().unwrap()
}

/// A smooth opaque gradient — what a lossy codec reproduces within a few levels.
fn gradient(width: u32, height: u32) -> Image {
    painted(width, height, |x, y| {
        [
            (x * 255 / width.max(2)) as u8,
            (y * 255 / height.max(2)) as u8,
            128,
            255,
        ]
    })
}

fn gif_bytes(image: &Image) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::gif::GifEncoder::new(&mut out)
        .encode(
            image.pixels(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgba8,
        )
        .expect("encode a GIF");
    out
}

fn max_difference(left: &Image, right: &Image) -> u8 {
    assert_eq!(
        (left.width(), left.height()),
        (right.width(), right.height())
    );
    left.pixels()
        .iter()
        .zip(right.pixels())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0)
}

// --- sniff ------------------------------------------------------------------

#[test]
fn sniff_names_the_four_formats_by_their_magic_bytes() {
    let picture = gradient(4, 4);
    for format in [Format::Png, Format::Jpeg, Format::Webp] {
        let bytes = picture.encode(format, 90).unwrap();
        assert_eq!(sniff(&bytes), Some(format), "{format:?}");
    }
    assert_eq!(sniff(&gif_bytes(&picture)), Some(Format::Gif));
    // The magic alone decides — a header with nothing after it still sniffs.
    assert_eq!(sniff(b"\x89PNG"), Some(Format::Png));
    assert_eq!(sniff(b"\xFF\xD8"), Some(Format::Jpeg));
    assert_eq!(sniff(b"RIFF\0\0\0\0WEBP"), Some(Format::Webp));
    assert_eq!(sniff(b"GIF89a"), Some(Format::Gif));
}

#[test]
fn sniff_refuses_what_is_not_a_picture() {
    assert_eq!(sniff(b""), None);
    assert_eq!(sniff(b"hello, world"), None);
    // A RIFF container that is not WebP (a WAV file) is not a picture.
    assert_eq!(sniff(b"RIFF\0\0\0\0WAVEfmt "), None);
    // Shorter than the WebP magic: not WebP, and nothing panics.
    assert_eq!(sniff(b"RIFF"), None);
    assert_eq!(sniff(b"\xFF"), None);
}

#[test]
fn the_seam_codes_round_trip_and_refuse_strangers() {
    for format in [Format::Png, Format::Jpeg, Format::Webp, Format::Gif] {
        assert_eq!(Format::from_code(format.code()), Some(format));
    }
    for fit in [Fit::Contain, Fit::Cover, Fit::Fill] {
        assert_eq!(Fit::from_code(fit.code()), Some(fit));
    }
    assert_eq!(
        [Format::Png, Format::Jpeg, Format::Webp, Format::Gif].map(Format::code),
        [0, 1, 2, 3]
    );
    assert_eq!(
        [Fit::Contain, Fit::Cover, Fit::Fill].map(Fit::code),
        [0, 1, 2]
    );
    assert_eq!(Format::from_code(4), None);
    assert_eq!(Format::from_code(-1), None);
    assert_eq!(Fit::from_code(3), None);
}

// --- decode -----------------------------------------------------------------

#[test]
fn a_png_decodes_to_exactly_its_pixels_alpha_kept() {
    let picture = painted(3, 2, |x, y| {
        [x as u8 * 80, y as u8 * 100, 7, 255 - x as u8 * 100]
    });
    let bytes = picture.encode(Format::Png, 0).unwrap();
    assert_eq!(Image::decode(&bytes, DEFAULT_MAX_PIXELS).unwrap(), picture);
}

#[test]
fn a_webp_decodes_to_exactly_its_pixels_alpha_kept() {
    let picture = painted(5, 3, |x, y| {
        [x as u8 * 50, y as u8 * 90, 200, 255 - y as u8 * 120]
    });
    let bytes = picture.encode(Format::Webp, 0).unwrap();
    assert_eq!(Image::decode(&bytes, DEFAULT_MAX_PIXELS).unwrap(), picture);
}

#[test]
fn a_jpeg_decodes_to_its_pixels_within_the_codecs_loss() {
    let picture = gradient(32, 24);
    let bytes = picture.encode(Format::Jpeg, 95).unwrap();
    let decoded = Image::decode(&bytes, DEFAULT_MAX_PIXELS).unwrap();
    assert!(
        max_difference(&decoded, &picture) <= 12,
        "{}",
        max_difference(&decoded, &picture)
    );
    assert!(
        decoded
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255)
    );
}

#[test]
fn a_gif_decodes_its_first_frame() {
    // A palette of four flat colours survives GIF's quantization exactly.
    let colours = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 255, 255],
    ];
    let picture = painted(4, 2, |x, _| colours[x as usize]);
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        let second = painted(4, 2, |_, _| [0, 0, 0, 255]);
        for frame in [&picture, &second] {
            encoder
                .encode_frame(image::Frame::new(frame.buffer.clone()))
                .expect("encode a frame");
        }
    }
    assert_eq!(Image::decode(&bytes, DEFAULT_MAX_PIXELS).unwrap(), picture);
}

#[test]
fn a_non_picture_is_refused_by_name() {
    assert_eq!(
        Image::decode(b"plain text", DEFAULT_MAX_PIXELS),
        Err("not a PNG, JPEG, WebP or GIF".to_string())
    );
}

#[test]
fn a_truncated_picture_is_refused_naming_its_format() {
    let bytes = gradient(16, 16).encode(Format::Png, 0).unwrap();
    let error = Image::decode(&bytes[..bytes.len() / 2], DEFAULT_MAX_PIXELS).unwrap_err();
    assert!(
        error.starts_with("the PNG could not be decoded: "),
        "{error}"
    );
}

/// A PNG's header (signature + IHDR + the start of IDAT) claiming `width` x `height`, and NOTHING
/// after it: no pixel data exists to decode, so a refusal that names the size
/// can only have come from the header.
fn png_header_only(width: u32, height: u32) -> Vec<u8> {
    let mut chunk = Vec::new();
    chunk.extend_from_slice(b"IHDR");
    chunk.extend_from_slice(&width.to_be_bytes());
    chunk.extend_from_slice(&height.to_be_bytes());
    chunk.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA, no interlace
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(&chunk);
    bytes.extend_from_slice(&crc32(&chunk).to_be_bytes());
    // The first IDAT's header and a zlib stream header — the PNG decoder reads
    // up to the image data before it answers its dimensions — then the file
    // ends: the pixel data itself is not there.
    let data = [0x78, 0x9c];
    bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut idat = b"IDAT".to_vec();
    idat.extend_from_slice(&data);
    bytes.extend_from_slice(&idat);
    bytes.extend_from_slice(&crc32(&idat).to_be_bytes());
    bytes
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[test]
fn the_pixel_limit_refuses_from_the_header_before_decoding() {
    // 60,000 x 60,000 is 14.4 GB of RGBA: had anything been allocated or
    // decoded, this would not be a sentence.
    assert_eq!(
        Image::decode(&png_header_only(60_000, 60_000), DEFAULT_MAX_PIXELS),
        Err("60000 x 60000 exceeds the 50,000,000-pixel limit".to_string())
    );
    assert_eq!(
        Image::decode(&png_header_only(4000, 4000), 1_000_000),
        Err("4000 x 4000 exceeds the 1,000,000-pixel limit".to_string())
    );
}

#[test]
fn the_pixel_limit_admits_a_picture_at_exactly_the_limit() {
    let bytes = gradient(10, 10).encode(Format::Png, 0).unwrap();
    assert!(Image::decode(&bytes, 100).is_ok());
    assert_eq!(
        Image::decode(&bytes, 99),
        Err("10 x 10 exceeds the 99-pixel limit".to_string())
    );
}

/// A minimal EXIF block (the APP1 payload the JPEG encoder writes after
/// `Exif\0\0`): a little-endian TIFF header and one IFD entry, Orientation
/// (0x0112, SHORT) = `orientation`.
fn exif_orientation(orientation: u16) -> Vec<u8> {
    let mut exif = b"II*\0".to_vec();
    exif.extend_from_slice(&8u32.to_le_bytes()); // the IFD follows the header
    exif.extend_from_slice(&1u16.to_le_bytes()); // one entry
    exif.extend_from_slice(&0x0112u16.to_le_bytes());
    exif.extend_from_slice(&3u16.to_le_bytes()); // SHORT
    exif.extend_from_slice(&1u32.to_le_bytes()); // one value
    exif.extend_from_slice(&orientation.to_le_bytes());
    exif.extend_from_slice(&[0, 0]); // the value's padding to four bytes
    exif.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    exif
}

#[test]
fn exif_orientation_six_comes_out_upright() {
    // A 2 x 3 picture (scaled by 8 so JPEG's 8 x 8 blocks keep the mark
    // crisp): white in its top-left cell, black elsewhere. Orientation 6 says
    // "rotate 90 degrees clockwise to view", so the upright picture is 3 x 2
    // and the mark lands top-RIGHT.
    let stored = painted(16, 24, |x, y| {
        if x < 8 && y < 8 {
            [255, 255, 255, 255]
        } else {
            [0, 0, 0, 255]
        }
    });
    let mut bytes = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut bytes, 100);
    encoder.set_exif_metadata(exif_orientation(6)).unwrap();
    encoder
        .write_image(
            &flattened_on_white(stored.pixels()),
            16,
            24,
            ExtendedColorType::Rgb8,
        )
        .unwrap();
    let upright = Image::decode(&bytes, DEFAULT_MAX_PIXELS).unwrap();
    assert_eq!((upright.width(), upright.height()), (24, 16));
    let bright = |x, y| pixel(&upright, x, y)[0] > 200;
    assert!(bright(20, 4), "the mark is top-right");
    assert!(!bright(4, 4), "and not where it was stored");
    assert!(!bright(20, 12) && !bright(4, 12));
}

#[test]
fn every_exif_orientation_is_applied() {
    // Orientation n against the transform the EXIF table gives it, on a
    // non-square picture so a swapped size shows.
    let stored = painted(16, 24, |x, y| {
        if x < 8 && y < 8 {
            [255, 255, 255, 255]
        } else {
            [0, 0, 0, 255]
        }
    });
    // (orientation, upright size, the 8 x 8 cell the mark lands in)
    let cases = [
        (1, (16, 24), (0, 0)),
        (2, (16, 24), (1, 0)),
        (3, (16, 24), (1, 2)),
        (4, (16, 24), (0, 2)),
        (5, (24, 16), (0, 0)),
        (6, (24, 16), (2, 0)),
        (7, (24, 16), (2, 1)),
        (8, (24, 16), (0, 1)),
    ];
    for (orientation, size, (cell_x, cell_y)) in cases {
        let mut bytes = Vec::new();
        let mut encoder = JpegEncoder::new_with_quality(&mut bytes, 100);
        encoder
            .set_exif_metadata(exif_orientation(orientation))
            .unwrap();
        encoder
            .write_image(
                &flattened_on_white(stored.pixels()),
                16,
                24,
                ExtendedColorType::Rgb8,
            )
            .unwrap();
        let upright = Image::decode(&bytes, DEFAULT_MAX_PIXELS).unwrap();
        assert_eq!(
            (upright.width(), upright.height()),
            size,
            "orientation {orientation}"
        );
        let bright = pixel(&upright, cell_x * 8 + 4, cell_y * 8 + 4)[0] > 200;
        assert!(
            bright,
            "orientation {orientation}: the mark is in cell ({cell_x}, {cell_y})"
        );
    }
}

// --- from_rgba / pixels -----------------------------------------------------

#[test]
fn from_rgba_refuses_a_length_mismatch() {
    assert_eq!(
        Image::from_rgba(2, 2, vec![0; 15]),
        Err("15 bytes are not 2 x 2 RGBA pixels (16 bytes)".to_string())
    );
    assert_eq!(
        Image::from_rgba(2, 2, vec![0; 17]),
        Err("17 bytes are not 2 x 2 RGBA pixels (16 bytes)".to_string())
    );
    // A product past u64 is refused rather than wrapped into a match.
    assert_eq!(
        Image::from_rgba(u64::MAX, 2, vec![]),
        Err(format!(
            "0 bytes are not {} x 2 RGBA pixels (too many bytes)",
            u64::MAX
        ))
    );
}

#[test]
fn from_rgba_round_trips_exactly() {
    let pixels: Vec<u8> = (0..24).collect();
    let image = Image::from_rgba(3, 2, pixels.clone()).unwrap();
    assert_eq!((image.width(), image.height()), (3, 2));
    assert_eq!(image.pixels(), &pixels[..]);
    assert_eq!(image.into_pixels(), pixels);
    // An empty picture is a picture.
    let empty = Image::from_rgba(0, 5, vec![]).unwrap();
    assert_eq!((empty.width(), empty.height()), (0, 5));
}

// --- resize -----------------------------------------------------------------

#[test]
fn contain_keeps_the_aspect_inside_the_box() {
    let picture = gradient(400, 300);
    let contained = picture.resized(100, 100, Fit::Contain).unwrap();
    assert_eq!((contained.width(), contained.height()), (100, 75));
    let tall = gradient(300, 400).resized(100, 100, Fit::Contain).unwrap();
    assert_eq!((tall.width(), tall.height()), (75, 100));
    // The bound side rounds to the nearest pixel: 3 x 2 into 4 wide is 2.67.
    let rounded = gradient(3, 2).resized(4, 100, Fit::Contain).unwrap();
    assert_eq!((rounded.width(), rounded.height()), (4, 3));
}

#[test]
fn contain_never_rounds_a_side_below_one_pixel() {
    let strip = gradient(1000, 1).resized(10, 10, Fit::Contain).unwrap();
    assert_eq!((strip.width(), strip.height()), (10, 1));
    let column = gradient(1, 1000).resized(10, 10, Fit::Contain).unwrap();
    assert_eq!((column.width(), column.height()), (1, 10));
}

#[test]
fn cover_and_fill_answer_exactly_the_box() {
    let picture = gradient(400, 300);
    for fit in [Fit::Cover, Fit::Fill] {
        let resized = picture.resized(64, 64, fit).unwrap();
        assert_eq!((resized.width(), resized.height()), (64, 64), "{fit:?}");
    }
    let upscaled = gradient(4, 4).resized(9, 5, Fit::Fill).unwrap();
    assert_eq!((upscaled.width(), upscaled.height()), (9, 5));
}

#[test]
fn cover_crops_the_overflow_centred() {
    // Three vertical bands, red | green | blue, each 10 wide: covering a
    // square takes the middle 10 x 10, so the result is all green.
    let bands = painted(30, 10, |x, _| match x / 10 {
        0 => [255, 0, 0, 255],
        1 => [0, 255, 0, 255],
        _ => [0, 0, 255, 255],
    });
    let covered = bands.resized(5, 5, Fit::Cover).unwrap();
    assert!(
        covered
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [0, 255, 0, 255]),
        "{:?}",
        covered.pixels()
    );
    // The same horizontally banded the other way up.
    let rows = painted(10, 30, |_, y| match y / 10 {
        0 => [255, 0, 0, 255],
        1 => [0, 0, 255, 255],
        _ => [255, 0, 0, 255],
    });
    let covered = rows.resized(5, 5, Fit::Cover).unwrap();
    assert!(
        covered
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [0, 0, 255, 255])
    );
}

#[test]
fn the_cover_window_centres_and_rounds() {
    assert_eq!(cover_window(400, 300, 100, 100), (50, 0, 300, 300));
    assert_eq!(cover_window(300, 400, 100, 100), (0, 50, 300, 300));
    // An odd leftover puts the extra pixel after the window.
    assert_eq!(cover_window(31, 10, 1, 1), (10, 0, 10, 10));
    // The box's aspect already: the whole picture.
    assert_eq!(cover_window(200, 100, 2, 1), (0, 0, 200, 100));
}

#[test]
fn fill_stretches_and_a_flat_colour_stays_flat() {
    let flat = painted(7, 3, |_, _| [10, 200, 30, 255]);
    let stretched = flat.resized(20, 2, Fit::Fill).unwrap();
    assert!(
        stretched
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [10, 200, 30, 255])
    );
    let shrunk = flat.resized(1, 1, Fit::Fill).unwrap();
    assert_eq!(shrunk.pixels(), &[10, 200, 30, 255]);
}

#[test]
fn a_resize_to_its_own_size_is_the_picture() {
    let picture = gradient(12, 9);
    assert_eq!(picture.resized(12, 9, Fit::Fill).unwrap(), picture);
    assert_eq!(picture.resized(12, 9, Fit::Contain).unwrap(), picture);
    assert_eq!(picture.resized(12, 9, Fit::Cover).unwrap(), picture);
}

#[test]
fn a_downscale_averages_rather_than_point_samples() {
    // A 1-pixel checkerboard halved: a point sample would answer pure black or
    // pure white; a low-pass filter answers the mean grey.
    let checker = painted(64, 64, |x, y| {
        if (x + y) % 2 == 0 {
            [255, 255, 255, 255]
        } else {
            [0, 0, 0, 255]
        }
    });
    let halved = checker.resized(16, 16, Fit::Fill).unwrap();
    for value in halved
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[0])
    {
        assert!((120..=135).contains(&value), "{value}");
    }
}

#[test]
fn transparent_colour_does_not_bleed_into_an_opaque_edge() {
    // Left half opaque white, right half fully transparent RED. Straight-alpha
    // resampling would tint the white edge pink; premultiplied, the red
    // carries no weight at all.
    let half = painted(16, 4, |x, _| {
        if x < 8 {
            [255, 255, 255, 255]
        } else {
            [255, 0, 0, 0]
        }
    });
    let resized = half.resized(5, 2, Fit::Fill).unwrap();
    for pixel in resized.pixels().as_chunks::<4>().0.iter() {
        if pixel[3] > 0 {
            assert_eq!(&pixel[..3], &[255, 255, 255], "{pixel:?}");
        } else {
            assert_eq!(*pixel, [0, 0, 0, 0]);
        }
    }
}

#[test]
fn a_zero_side_of_the_box_is_one_pixel_and_an_empty_picture_stays_empty() {
    let picture = gradient(8, 8);
    let squeezed = picture.resized(0, 4, Fit::Fill).unwrap();
    assert_eq!((squeezed.width(), squeezed.height()), (1, 4));
    let empty = Image::from_rgba(0, 3, vec![]).unwrap();
    assert_eq!(empty.resized(10, 10, Fit::Cover).unwrap(), empty);
}

#[test]
fn the_sine_the_kernel_uses_is_the_sine() {
    for step in -300..=300 {
        let x = f64::from(step) / 100.0;
        let exact = (std::f64::consts::PI * x).sin();
        assert!((sin_pi(x) - exact).abs() < 1e-11, "sin(pi * {x})");
    }
    assert_eq!(lanczos3(0.0), 1.0);
    assert_eq!(lanczos3(3.0), 0.0);
    assert_eq!(lanczos3(-4.5), 0.0);
}

// --- crop -------------------------------------------------------------------

#[test]
fn crop_reads_the_window() {
    let picture = painted(4, 3, |x, y| [x as u8, y as u8, 0, 255]);
    let window = picture.cropped(1, 1, 2, 2);
    assert_eq!((window.width(), window.height()), (2, 2));
    assert_eq!(pixel(&window, 0, 0), [1, 1, 0, 255]);
    assert_eq!(pixel(&window, 1, 1), [2, 2, 0, 255]);
}

#[test]
fn crop_clamps_rather_than_refuses() {
    let picture = painted(4, 3, |x, y| [x as u8, y as u8, 0, 255]);
    // Past the right and bottom edges: stops at them.
    let window = picture.cropped(2, 1, 100, 100);
    assert_eq!((window.width(), window.height()), (2, 2));
    assert_eq!(pixel(&window, 1, 1), [3, 2, 0, 255]);
    // Starting past an edge: empty.
    let outside = picture.cropped(10, 0, 5, 5);
    assert_eq!((outside.width(), outside.height()), (0, 3));
    assert!(outside.pixels().is_empty());
    let below = picture.cropped(0, 3, 5, 5);
    assert_eq!((below.width(), below.height()), (4, 0));
}

// --- encode -----------------------------------------------------------------

#[test]
fn png_and_webp_round_trip_exactly() {
    let picture = painted(9, 7, |x, y| {
        [x as u8 * 28, y as u8 * 36, (x * y) as u8, (x * 25) as u8]
    });
    for format in [Format::Png, Format::Webp] {
        let bytes = picture.encode(format, 50).unwrap();
        assert_eq!(
            Image::decode(&bytes, DEFAULT_MAX_PIXELS).unwrap(),
            picture,
            "{format:?}"
        );
    }
}

#[test]
fn jpeg_round_trips_within_a_tolerance_and_quality_moves_the_size() {
    let picture = gradient(64, 48);
    let fine = picture.encode(Format::Jpeg, 95).unwrap();
    let coarse = picture.encode(Format::Jpeg, 10).unwrap();
    assert!(
        coarse.len() < fine.len(),
        "{} vs {}",
        coarse.len(),
        fine.len()
    );
    let decoded = Image::decode(&fine, DEFAULT_MAX_PIXELS).unwrap();
    assert!(max_difference(&decoded, &picture) <= 12);
    // Out-of-range qualities clamp rather than refuse.
    assert!(picture.encode(Format::Jpeg, 0).is_ok());
    assert!(picture.encode(Format::Jpeg, 1000).is_ok());
}

#[test]
fn jpeg_flattens_transparency_onto_white() {
    let transparent = painted(16, 16, |_, _| [0, 0, 0, 0]);
    let decoded = Image::decode(
        &transparent.encode(Format::Jpeg, 95).unwrap(),
        DEFAULT_MAX_PIXELS,
    )
    .unwrap();
    assert!(
        decoded
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[0] >= 250 && pixel[3] == 255)
    );
    // Half-transparent black is mid grey.
    assert_eq!(
        flattened_on_white(&[0, 0, 0, 128, 255, 0, 0, 255]),
        vec![127, 127, 127, 255, 0, 0]
    );
}

#[test]
fn gif_cannot_be_encoded() {
    assert_eq!(
        gradient(2, 2).encode(Format::Gif, 80),
        Err("GIF cannot be encoded".to_string())
    );
}

#[test]
fn an_empty_picture_is_refused_by_the_encoder_not_a_panic() {
    let empty = Image::from_rgba(0, 0, vec![]).unwrap();
    for format in [Format::Png, Format::Jpeg, Format::Webp] {
        if let Err(error) = empty.encode(format, 80) {
            assert!(error.contains(format.name()), "{error}");
        }
    }
}

#[test]
fn thousands_group_by_commas() {
    assert_eq!(grouped(0), "0");
    assert_eq!(grouped(999), "999");
    assert_eq!(grouped(1000), "1,000");
    assert_eq!(grouped(50_000_000), "50,000,000");
    assert_eq!(grouped(123_456_789_012), "123,456,789,012");
}

#[test]
fn a_result_over_the_limit_is_refused_naming_both_sizes() {
    let picture = gradient(4, 3);
    assert_eq!(
        picture.resized(100_000, 100_000, Fit::Fill),
        Err("a 100000 x 100000 result exceeds the 50,000,000-pixel limit".to_string())
    );
    assert_eq!(
        picture.resized(100_000, 100_000, Fit::Cover),
        Err("a 100000 x 100000 result exceeds the 50,000,000-pixel limit".to_string())
    );
    // `Contain` measures what it would answer, not the box: 4 x 3 into a
    // 100,000 square is 100,000 x 75,000.
    assert_eq!(
        picture.resized(100_000, 100_000, Fit::Contain),
        Err("a 100000 x 75000 result exceeds the 50,000,000-pixel limit".to_string())
    );
    // A box side past `u32` is reported as asked, not wrapped or clamped.
    assert_eq!(
        picture.resized(u64::MAX, 1, Fit::Fill),
        Err(format!(
            "a {} x 1 result exceeds the 50,000,000-pixel limit",
            u64::MAX
        ))
    );
}

#[test]
fn the_limit_is_on_the_result_so_a_huge_box_can_still_contain_a_strip() {
    // 1 x 10,000 into a 100,000 square: Contain answers 10 x 100,000 =
    // 1,000,000 pixels, inside the limit though the box is far past it.
    let strip = gradient(1, 10_000);
    let contained = strip.resized(100_000, 100_000, Fit::Contain).unwrap();
    assert_eq!((contained.width(), contained.height()), (10, 100_000));
    // One pixel past the limit is refused (and nothing is allocated for it).
    assert_eq!(
        gradient(1, 1).resized(10_000, 5_001, Fit::Fill),
        Err("a 10000 x 5001 result exceeds the 50,000,000-pixel limit".to_string())
    );
}
