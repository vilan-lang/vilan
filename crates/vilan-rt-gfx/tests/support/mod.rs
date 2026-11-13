//! What the GPU tests share: an adapter or a printed reason to skip, the
//! fixed test font, and golden-image comparison.
//!
//! A machine with no software adapter SKIPS these tests, printing why, so a
//! developer without Mesa can still run the crate's suite; CI's `gfx` job sets
//! `VILAN_REQUIRE_GFX=1`, which turns every skip into a failure, so the
//! goldens can never pass there by not running.

#![allow(
    dead_code,
    reason = "each test binary uses its own part of the support module"
)]

pub mod icons;

use std::path::PathBuf;

use vilan_rt_gfx::{AdapterPreference, GpuContext, TextSystem};

/// The environment variable that makes a missing adapter (or display) a
/// failure instead of a skip.
pub const REQUIRE: &str = "VILAN_REQUIRE_GFX";

/// Rewrites the goldens from this run's output instead of comparing. Look at
/// every image it writes before committing it.
pub const REGENERATE: &str = "VILAN_REGENERATE_GFX_GOLDENS";

/// Software Vulkan (lavapipe) and software GL (llvmpipe) differ by one in a
/// handful of channel values; nothing else is tolerated.
pub const TOLERANCE: u8 = 1;

pub fn required() -> bool {
    std::env::var_os(REQUIRE).is_some_and(|value| value != "0")
}

/// Skips (returns `None`, printing why) or, under [`REQUIRE`], fails.
pub fn skip_or_fail(reason: &str) {
    if required() {
        panic!("{reason}, and {REQUIRE} is set: this test must run here");
    }
    eprintln!("SKIPPED: {reason} (set {REQUIRE}=1 to make this a failure)");
}

/// A software adapter, the one the goldens were drawn with, or `None` after
/// [`skip_or_fail`].
pub fn software_context() -> Option<GpuContext> {
    match GpuContext::headless(AdapterPreference::Software) {
        Ok(context) => {
            let adapter = context.adapter();
            eprintln!(
                "adapter: {} ({}, {}, {})",
                adapter.name, adapter.backend, adapter.device_type, adapter.driver
            );
            Some(context)
        }
        Err(error) => {
            skip_or_fail(&format!(
                "no software GPU adapter ({error}); install Mesa's lavapipe (mesa-vulkan-drivers)"
            ));
            None
        }
    }
}

pub fn crate_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A text system that sees only the test font, Noto Mono (OFL-1.1, see
/// `tests/fonts/OFL.txt`), so text renders the same on every machine.
pub fn test_text_system() -> TextSystem {
    let mut text = TextSystem::without_system_fonts();
    let font = std::fs::read(crate_directory().join("tests/fonts/NotoMono-Regular.ttf"))
        .expect("the test font is committed");
    let families = text.register_font(font).expect("Noto Mono registers");
    assert_eq!(families, vec!["Noto Mono".to_string()]);
    text
}

pub fn read_png(path: &std::path::Path) -> (u32, u32, Vec<u8>) {
    let file = std::fs::File::open(path)
        .unwrap_or_else(|error| panic!("open {}: {error}", path.display()));
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().expect("a PNG header");
    let mut pixels = vec![0; reader.output_buffer_size().expect("a bounded image")];
    let info = reader.next_frame(&mut pixels).expect("PNG data");
    assert_eq!(
        (info.color_type, info.bit_depth),
        (png::ColorType::Rgba, png::BitDepth::Eight),
        "{} is not 8-bit RGBA",
        path.display()
    );
    pixels.truncate(info.buffer_size());
    (info.width, info.height, pixels)
}

pub fn write_png(path: &std::path::Path, width: u32, height: u32, pixels: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create the directory");
    }
    let file = std::fs::File::create(path)
        .unwrap_or_else(|error| panic!("create {}: {error}", path.display()));
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .expect("PNG header")
        .write_image_data(pixels)
        .expect("PNG data");
}

/// Holds `pixels` to `tests/golden/<name>.png` within [`TOLERANCE`], or
/// rewrites the golden under [`REGENERATE`]. On a mismatch the frame is
/// written beside the build's target directory for inspection and the test
/// fails naming it.
pub fn assert_golden(name: &str, width: u32, height: u32, pixels: &[u8]) {
    assert_golden_allowing(name, width, height, pixels, 0);
}

/// What a comparison found: how many channel values are past [`TOLERANCE`],
/// the largest difference, and the first pixel past it.
#[derive(Debug, PartialEq, Eq)]
pub struct Differences {
    pub differing: usize,
    pub largest: u8,
    pub first: Option<(u32, u32)>,
}

/// Compares two RGBA frames of one size, channel by channel.
pub fn differences(pixels: &[u8], expected: &[u8], width: u32) -> Differences {
    let mut found = Differences {
        differing: 0,
        largest: 0,
        first: None,
    };
    for (index, (actual, wanted)) in pixels.iter().zip(expected).enumerate() {
        let difference = actual.abs_diff(*wanted);
        if difference > TOLERANCE {
            found.differing += 1;
            if found.first.is_none() {
                let pixel = (index / 4) as u32;
                found.first = Some((pixel % width, pixel / width));
            }
        }
        found.largest = found.largest.max(difference);
    }
    found
}

/// Whether a comparison passes with `stragglers` allowed: at most that many
/// channel values past [`TOLERANCE`], and none of them by more than one
/// further. With no stragglers allowed this is [`TOLERANCE`] alone.
pub fn within(found: &Differences, stragglers: usize) -> bool {
    found.differing == 0 || (found.differing <= stragglers && found.largest <= TOLERANCE + 1)
}

/// [`assert_golden`], with up to `stragglers` channel values allowed to be
/// one further off than [`TOLERANCE`] (and none further than that).
///
/// For frames whose edges are coverage blended over a background and then
/// encoded to sRGB: two versions of one software rasteriser round the blend's
/// last bit differently, and the encode stretches that bit to two at a dark
/// value. CI's Mesa 25.2.8 put ONE pixel of `paths-125` (three channel
/// values) two off a golden made on Mesa 25.0.7 (run 43492572577), with every
/// other pixel of the three sheets inside the tolerance.
pub fn assert_golden_allowing(
    name: &str,
    width: u32,
    height: u32,
    pixels: &[u8],
    stragglers: usize,
) {
    let golden = crate_directory()
        .join("tests/golden")
        .join(format!("{name}.png"));
    if std::env::var_os(REGENERATE).is_some() {
        write_png(&golden, width, height, pixels);
        eprintln!("wrote {}", golden.display());
        return;
    }
    assert!(
        golden.is_file(),
        "no golden at {} (run with {REGENERATE}=1, then LOOK at the image)",
        golden.display()
    );
    let (golden_width, golden_height, expected) = read_png(&golden);
    assert_eq!(
        (golden_width, golden_height),
        (width, height),
        "{name}: size differs from the golden"
    );
    let found = differences(pixels, &expected, width);
    if !within(&found, stragglers) {
        let Differences {
            differing,
            largest,
            first,
        } = found;
        let failure = crate_directory()
            .join("target/golden-failures")
            .join(format!("{name}.png"));
        write_png(&failure, width, height, pixels);
        panic!(
            "{name}: {differing} channel values differ from the golden by more than {TOLERANCE} \
             (largest {largest}, first at pixel {first:?}; {stragglers} allowed at {} and none \
             past it); this run's frame is at {}",
            TOLERANCE + 1,
            failure.display()
        );
    }
}
