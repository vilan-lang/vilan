//! Paths on the GPU (gfx-path.md S1): golden images of the paper's seven
//! icons at 16, 24, 48 and 256 dp, at 100%, 125% and 150%, held to the
//! goldens within one per channel, as the text goldens are; and a path
//! placed inside a nested scene drawing the pixels it draws in window
//! coordinates.
//!
//! Skipped with a printed reason where there is no software adapter;
//! required under `VILAN_REQUIRE_GFX=1` (CI's `gfx` job). See
//! `support/mod.rs`.

mod support;

use support::icons::{ICON_BOX, ICONS, IconPaint, SIZES, STROKE_WIDTH};
use support::{
    Differences, assert_golden_allowing, differences, software_context, test_text_system, within,
};
use vilan_rt_gfx::{
    Color, FillRule, OffscreenRenderer, Path, PathPaint, Point, Scene, SceneBuilder, Stroke, Units,
};

/// The icons' colour, sRGB: a dark slate, as text would be (the spike's).
const INK: Color = Color::new(31.0 / 255.0, 41.0 / 255.0, 55.0 / 255.0, 1.0);
const BACKGROUND: Color = Color::new(245.0 / 255.0, 245.0 / 255.0, 240.0 / 255.0, 1.0);

/// The gap between icons, in dp.
const GAP: f32 = 12.0;

fn paint(paint: IconPaint) -> PathPaint {
    match paint {
        IconPaint::Stroke => PathPaint::Stroke(Stroke::new(STROKE_WIDTH)),
        IconPaint::Fill => PathPaint::Fill(FillRule::NonZero),
        IconPaint::FillEvenOdd => PathPaint::Fill(FillRule::EvenOdd),
    }
}

/// Each column's left edge, in dp: one column per size.
fn columns() -> [f32; 4] {
    let mut left = GAP;
    SIZES.map(|size| {
        let column = left;
        left += size + GAP;
        column
    })
}

fn sheet_size() -> (f32, f32) {
    let width = SIZES.iter().map(|size| size + GAP).sum::<f32>() + GAP;
    let row = SIZES[3] + GAP;
    (width, ICONS.len() as f32 * row + GAP)
}

/// One row per icon, one column per size, each icon's box with its top-left
/// corner on the grid of dp.
fn sheet() -> Scene {
    let mut scene = SceneBuilder::new();
    for (row, icon) in ICONS.iter().enumerate() {
        let path = Path::from_svg(icon.data).expect("the icons parse");
        let top = GAP + row as f32 * (SIZES[3] + GAP);
        for (size, left) in SIZES.iter().zip(columns()) {
            scene.path(
                &path,
                Point::new(left, top),
                size / ICON_BOX,
                paint(icon.paint),
                INK,
            );
        }
    }
    scene.finish().expect("a balanced scene")
}

#[test]
fn the_seven_icons_match_their_goldens_at_every_size_and_scale() {
    let (width_dp, height_dp) = sheet_size();
    for scale_factor in [1.0f32, 1.25, 1.5] {
        let Some(context) = software_context() else {
            return;
        };
        let mut text = test_text_system();
        let width = (width_dp * scale_factor).round() as u32;
        let height = (height_dp * scale_factor).round() as u32;
        let mut target = OffscreenRenderer::new(context, width, height).expect("a target");
        let units = Units::new(scale_factor, 1.0).expect("valid units");
        let stats = target
            .render(&sheet(), units, &mut text, BACKGROUND)
            .expect("the frame renders");
        assert_eq!(stats.culled_primitives, 0, "every icon is drawn");
        assert_eq!(stats.primitives, 28);
        // A mask over 256 device pixels a side gets a texture of its own
        // (R8): none at 100% (a 256 dp icon's ink is inside its 256-pixel
        // box); at 125% the five whose ink is 20 units or more across (all
        // but the check, 18 units with its stroke, and the chevron, 14); at
        // 150% the check too. Every smaller mask shares one atlas page.
        let own = match (scale_factor * 100.0).round() as u32 {
            100 => 0,
            125 => 5,
            _ => 6,
        };
        assert_eq!(stats.path_mask_textures, own, "at {scale_factor}");
        assert_eq!(stats.atlas.mask_pages, 1, "at {scale_factor}");
        let pixels = target.read_rgba().expect("read back");
        assert_golden_allowing(
            &format!("paths-{}", (scale_factor * 100.0).round() as u32),
            width,
            height,
            &pixels,
            EDGE_STRAGGLERS,
        );
    }
}

/// How many channel values of a sheet may be one further off than the
/// tolerance: four pixels' worth. A sheet has some 30,000 edge pixels, each a
/// coverage blended and then encoded to sRGB, and two versions of Mesa's
/// software rasteriser disagreed on one of them by two (`assert_golden_allowing`
/// has the run). A wrong mask moves hundreds of values, or one by far more.
const EDGE_STRAGGLERS: usize = 12;

/// The allowance is what it says: a few values one further off pass, one more
/// than the allowance does not, a value two further off never does, and with
/// no allowance the tolerance stands alone.
#[test]
fn a_few_edge_values_one_further_off_pass_and_nothing_else_does() {
    let golden = vec![100u8; 4 * 8];
    let off = |values: &[(usize, u8)]| {
        let mut frame = golden.clone();
        for (index, by) in values {
            frame[*index] += by;
        }
        differences(&frame, &golden, 4)
    };
    assert_eq!(
        off(&[(0, 1), (5, 1)]),
        Differences {
            differing: 0,
            largest: 1,
            first: None
        },
        "inside the tolerance"
    );
    assert!(within(&off(&[(0, 1), (5, 1)]), 0));
    // CI's case: one pixel's three colour values, two off.
    let one_pixel = off(&[(20, 2), (21, 2), (22, 2)]);
    assert_eq!(one_pixel.first, Some((1, 1)));
    assert!(within(&one_pixel, EDGE_STRAGGLERS));
    assert!(!within(&one_pixel, 0), "the plain tolerance refuses it");
    assert!(!within(&one_pixel, 2), "one value more than the allowance");
    // Two further off is never a straggler, however few.
    assert!(!within(&off(&[(20, 3)]), EDGE_STRAGGLERS));
    // More of them than the allowance: a real change.
    let many: Vec<(usize, u8)> = (0..=EDGE_STRAGGLERS).map(|index| (index, 2)).collect();
    assert!(!within(&off(&many), EDGE_STRAGGLERS));
}

/// A path in a scene placed by `scene_at` (how `std::ui` places an
/// element's scene) draws exactly the pixels the same path draws in window
/// coordinates at the same place, at a fractional origin and scale.
#[test]
fn a_path_in_a_placed_scene_draws_the_window_coordinates_pixels() {
    let Some(context) = software_context() else {
        return;
    };
    let scale_factor = 1.25;
    let star = Path::from_svg(ICONS[2].data).expect("the star parses");
    let origin = Point::new(2.3, 3.6);
    let mut inner = SceneBuilder::new();
    inner.path(&star, origin, 0.9, paint(IconPaint::Stroke), INK);
    let inner = inner.finish().expect("balanced");
    // The placed copy's offset is a snapped box's corner: whole device
    // pixels (40, 0) as dp.
    let offset = Point::new(32.0, 0.0);
    let mut scene = SceneBuilder::new();
    scene.path(&star, origin, 0.9, paint(IconPaint::Stroke), INK);
    scene.scene_at(&inner, offset);
    let scene = scene.finish().expect("balanced");
    let (width, height) = (80u32, 40u32);
    let mut target = OffscreenRenderer::new(context, width, height).expect("a target");
    let mut text = test_text_system();
    target
        .render(
            &scene,
            Units::new(scale_factor, 1.0).expect("valid"),
            &mut text,
            BACKGROUND,
        )
        .expect("the frame renders");
    let pixels = target.read_rgba().expect("read back");
    let shift = (offset.x * scale_factor) as u32;
    let mut inked = 0;
    for y in 0..height {
        for x in 0..shift {
            let at = |x: u32| {
                let start = ((y * width + x) * 4) as usize;
                pixels[start..start + 4].to_vec()
            };
            assert_eq!(at(x), at(x + shift), "pixel ({x}, {y})");
            if at(x)[0] < 200 {
                inked += 1;
            }
        }
    }
    assert!(inked > 20, "the star is drawn ({inked} dark pixels)");
}
