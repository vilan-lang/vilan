//! The renderer, offscreen, on a software adapter: golden images of the
//! primitive set at four scales, and decoded-pixel pins of paint order (R14),
//! hairlines (R16) and the atlas under eviction.
//!
//! Skipped with a printed reason where there is no software adapter; required
//! under `VILAN_REQUIRE_GFX=1` (CI's `gfx` job). See `support/mod.rs`.

mod support;

use support::{assert_golden, software_context, test_text_system};
use vilan_rt_gfx::{
    AtlasLimits, Border, Color, Corners, Edges, FontFamily, FrameStats, Image, LinearGradient,
    OffscreenRenderer, Point, Rect, Rectangle, Scene, SceneBuilder, Shadow, TextStyle, TextSystem,
    Transform, Underline, Units,
};

const SCALES: [f32; 4] = [1.0, 1.25, 1.5, 2.0];

fn mono(size: f32) -> TextStyle {
    TextStyle {
        family: FontFamily::Named("Noto Mono".to_string()),
        size,
        ..TextStyle::default()
    }
}

/// An 8 x 8 checkerboard of two colours, one pixel per square, with a
/// transparent top-left quarter whose colour channels are MAGENTA, and a
/// half-transparent last row. Filtering straight alpha would bleed the
/// invisible magenta into the visible squares beside it; the renderer
/// premultiplies before it filters, so no magenta may appear.
fn checkerboard() -> Image {
    let mut pixels = Vec::with_capacity(8 * 8 * 4);
    for row in 0..8 {
        for column in 0..8 {
            let color: [u8; 4] = if row < 4 && column < 4 {
                [255, 0, 255, 0]
            } else if row == 7 {
                [30, 140, 90, 128]
            } else if (row + column) % 2 == 0 {
                [30, 140, 90, 255]
            } else {
                [250, 210, 60, 255]
            };
            pixels.extend_from_slice(&color);
        }
    }
    Image::from_rgba(8, 8, pixels).expect("64 RGBA pixels")
}

/// Every primitive, every group, in one 320 x 200 dp scene.
fn primitives_scene(text: &mut TextSystem) -> Scene {
    let mut scene = SceneBuilder::new();
    let card = Rect::new(20.0, 20.0, 130.0, 80.0);
    scene.shadow(Shadow {
        bounds: Rect::new(20.0, 24.0, 130.0, 80.0),
        corner_radius: 10.0,
        blur: 12.0,
        color: Color::rgba(0, 0, 0, 90),
    });
    scene.rectangle(
        Rectangle::new(card, Color::WHITE)
            .with_corner_radii(Corners {
                top_left: 12.0,
                top_right: 4.0,
                bottom_right: 12.0,
                bottom_left: 4.0,
            })
            .with_border(Border {
                widths: Edges {
                    top: 1.0,
                    right: 1.0,
                    bottom: 3.0,
                    left: 1.0,
                },
                color: Color::rgb(150, 150, 170),
            }),
    );
    let greeting = text.layout("Hello, gfx", &mono(14.0), None);
    scene.text(&greeting, Point::new(32.0, 30.0), Color::rgb(30, 30, 40));
    scene.underline(Underline {
        origin: Point::new(32.0, 30.0 + greeting.first_baseline() + 2.0),
        width: greeting.size().width,
        thickness: 1.0,
        color: Color::rgb(200, 40, 40),
    });
    let wrapped = text.layout("wrapped onto two lines", &mono(10.0), Some(110.0));
    scene.text(&wrapped, Point::new(32.0, 60.0), Color::rgb(90, 90, 100));

    scene.rectangle(
        Rectangle::new(
            Rect::new(170.0, 20.0, 130.0, 40.0),
            LinearGradient {
                start: Point::new(170.0, 20.0),
                end: Point::new(300.0, 20.0),
                start_color: Color::rgb(255, 120, 0),
                end_color: Color::rgb(40, 90, 255),
            },
        )
        .with_radius(20.0),
    );

    let image = checkerboard();
    scene.push_clip(Rect::new(170.0, 75.0, 130.0, 50.0), Corners::all(16.0));
    scene.image(&image, Rect::new(170.0, 75.0, 130.0, 50.0), Corners::ZERO);
    scene.rectangle(Rectangle::new(
        Rect::new(270.0, 100.0, 60.0, 60.0),
        Color::rgba(200, 0, 120, 200),
    ));
    scene.pop();
    scene.image(
        &image,
        Rect::new(20.0, 120.0, 60.0, 60.0),
        Corners::all(30.0),
    );

    scene.push_transform(Transform::translate(100.0, 120.0));
    scene.push_transform(Transform::scale(1.5));
    scene.rectangle(
        Rectangle::new(Rect::new(0.0, 0.0, 30.0, 30.0), Color::rgb(60, 60, 70))
            .with_radius(6.0)
            .with_border(Border::all(1.0, Color::rgb(250, 250, 250))),
    );
    let scaled = text.layout("x1.5", &mono(8.0), None);
    scene.text(&scaled, Point::new(4.0, 9.0), Color::WHITE);
    scene.pop();
    scene.pop();

    scene.push_layer(0.5);
    scene.rectangle(
        Rectangle::new(Rect::new(220.0, 135.0, 80.0, 50.0), Color::rgb(20, 60, 200))
            .with_radius(8.0),
    );
    let half = text.layout("50%", &mono(16.0), None);
    scene.text(&half, Point::new(240.0, 150.0), Color::WHITE);
    scene.pop();
    scene.finish().expect("a balanced scene")
}

fn device_size(width: f32, height: f32, scale_factor: f32) -> (u32, u32) {
    (
        (width * scale_factor).round() as u32,
        (height * scale_factor).round() as u32,
    )
}

fn render(
    context_target: &mut OffscreenRenderer,
    scene: &Scene,
    scale_factor: f32,
    text: &mut TextSystem,
) -> (FrameStats, Vec<u8>) {
    let units = Units::new(scale_factor, 1.0).expect("valid units");
    let stats = context_target
        .render(scene, units, text, Color::rgb(245, 245, 240))
        .expect("the frame renders");
    (stats, context_target.read_rgba().expect("read back"))
}

fn scale_name(scale_factor: f32) -> String {
    format!("{}", (scale_factor * 100.0).round() as u32)
}

#[test]
fn the_primitive_set_matches_its_golden_at_every_scale() {
    for scale_factor in SCALES {
        let Some(context) = software_context() else {
            return;
        };
        let mut text = test_text_system();
        let scene = primitives_scene(&mut text);
        let (width, height) = device_size(320.0, 200.0, scale_factor);
        let mut target = OffscreenRenderer::new(context, width, height).expect("a target");
        let (stats, pixels) = render(&mut target, &scene, scale_factor, &mut text);
        assert_eq!(
            stats.culled_primitives, 0,
            "every primitive of the scene is visible"
        );
        assert_golden(
            &format!("primitives-{}", scale_name(scale_factor)),
            width,
            height,
            &pixels,
        );
    }
}

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let start = ((y * width + x) * 4) as usize;
    [
        pixels[start],
        pixels[start + 1],
        pixels[start + 2],
        pixels[start + 3],
    ]
}

fn near(actual: [u8; 4], expected: [u8; 4]) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(actual, expected)| actual.abs_diff(expected) <= support::TOLERANCE)
}

const RED: [u8; 4] = [220, 30, 30, 255];

/// Text, then a rectangle over part of it: R14's ordering half, by pixels and
/// by draw calls. The control moves the rectangle off the text and finds text
/// pixels where the rectangle was, so the first half is not vacuous.
#[test]
fn a_later_rectangle_over_earlier_text_covers_it() {
    let Some(context) = software_context() else {
        return;
    };
    let mut text = test_text_system();
    let word = text.layout("MMMM", &mono(28.0), None);
    let cover = Rect::new(20.0, 12.0, 30.0, 26.0);
    let build = |rectangle_at: Rect| {
        let mut scene = SceneBuilder::new();
        scene.rectangle(Rectangle::new(
            Rect::new(0.0, 0.0, 200.0, 60.0),
            Color::WHITE,
        ));
        scene.text(&word, Point::new(10.0, 10.0), Color::BLACK);
        scene.rectangle(Rectangle::new(
            rectangle_at,
            Color::rgb(RED[0], RED[1], RED[2]),
        ));
        scene.finish().expect("balanced")
    };
    let mut target = OffscreenRenderer::new(context, 200, 60).expect("a target");

    let (stats, pixels) = render(&mut target, &build(cover), 1.0, &mut text);
    assert_eq!(
        stats.draw_calls, 3,
        "background, text, then the rectangle over it"
    );
    for y in 12..38 {
        for x in 20..50 {
            let found = pixel(&pixels, 200, x, y);
            assert!(
                near(found, RED),
                "({x}, {y}) is {found:?}: text drew over the later rectangle"
            );
        }
    }

    let (stats, pixels) = render(
        &mut target,
        &build(Rect::new(150.0, 12.0, 30.0, 26.0)),
        1.0,
        &mut text,
    );
    assert_eq!(
        stats.draw_calls, 2,
        "no overlap across kinds: one draw for the rectangles, one for the text"
    );
    let dark = (12..38)
        .flat_map(|y| (20..50).map(move |x| (x, y)))
        .filter(|(x, y)| pixel(&pixels, 200, *x, *y)[0] < 100)
        .count();
    assert!(
        dark > 50,
        "the control finds the text under where the rectangle was ({dark} dark pixels)"
    );
}

/// R14's batching half: twenty rectangles and twenty text runs interleaved in
/// the scene, none overlapping the other kind, are two draw calls.
#[test]
fn non_overlapping_rectangles_and_text_are_two_draw_calls() {
    let Some(context) = software_context() else {
        return;
    };
    let mut text = test_text_system();
    let label = text.layout("row", &mono(10.0), None);
    let mut scene = SceneBuilder::new();
    for row in 0..20 {
        let y = row as f32 * 15.0;
        scene.rectangle(
            Rectangle::new(Rect::new(0.0, y, 40.0, 12.0), Color::rgb(100, 140, 220))
                .with_radius(3.0),
        );
        scene.text(&label, Point::new(50.0, y), Color::BLACK);
    }
    let scene = scene.finish().expect("balanced");
    let mut target = OffscreenRenderer::new(context, 120, 300).expect("a target");
    let (stats, pixels) = render(&mut target, &scene, 1.0, &mut text);
    assert_eq!(stats.draw_calls, 2);
    assert_eq!((stats.quad_instances, stats.primitives), (20, 40));
    // Every row drew both: a rectangle pixel and a text pixel per row.
    for row in 0..20 {
        let y = row * 15 + 6;
        assert!(
            near(pixel(&pixels, 120, 20, y), [100, 140, 220, 255]),
            "row {row}'s rectangle"
        );
        let inked = (50..80).any(|x| (y - 4..y + 4).any(|y| pixel(&pixels, 120, x, y)[0] < 128));
        assert!(inked, "row {row}'s text");
    }
}

/// Rows of the run of dark pixels starting at `y` in column `x`.
fn dark_run(pixels: &[u8], width: u32, x: u32, from_y: u32) -> u32 {
    (from_y..)
        .take_while(|y| pixel(pixels, width, x, *y)[0] < 128)
        .count() as u32
}

/// The first dark row in `rows` of column `x`. Panics when there is none: a
/// line that was not drawn must not be found somewhere else below it.
fn first_dark(pixels: &[u8], width: u32, x: u32, rows: std::ops::Range<u32>) -> u32 {
    let searched = rows.clone();
    rows.into_iter()
        .find(|y| pixel(pixels, width, x, *y)[0] < 128)
        .unwrap_or_else(|| panic!("no dark pixel in column {x}, rows {searched:?}"))
}

/// R16 on decoded pixels: a 1 dp border is one fully opaque device pixel at
/// 100%, 125% and 150%, two at 200%; a 2 dp border rounds to the nearest
/// pixel; a 0.3 dp border is still one pixel; a 1 dp divider (a rectangle, not
/// a border) is the same height wherever it sits. Each line is sharp: black
/// against white, nothing blended.
#[test]
fn hairlines_are_whole_device_pixels_at_every_scale() {
    let expected = [
        // (scale, 1 dp, 2 dp, 0.3 dp, divider)
        (1.0, 1, 2, 1, 1),
        (1.25, 1, 3, 1, 1),
        (1.5, 1, 3, 1, 1),
        (2.0, 2, 4, 1, 2),
    ];
    for (scale_factor, one, two, thin, divider) in expected {
        let Some(context) = software_context() else {
            return;
        };
        let mut text = test_text_system();
        let mut scene = SceneBuilder::new();
        scene.rectangle(Rectangle::new(
            Rect::new(0.0, 0.0, 200.0, 100.0),
            Color::WHITE,
        ));
        // Fractional positions on purpose: the snap, not the input, decides.
        for (index, width) in [1.0, 2.0, 0.3].into_iter().enumerate() {
            let left = 10.4 + index as f32 * 60.0;
            scene.rectangle(
                Rectangle::new(Rect::new(left, 10.6, 50.0, 40.0), Color::WHITE)
                    .with_border(Border::all(width, Color::BLACK)),
            );
        }
        for (index, top) in [60.3, 70.7, 80.5].into_iter().enumerate() {
            scene.rectangle(Rectangle::new(
                Rect::new(10.0 + index as f32 * 60.0, top, 50.0, 1.0),
                Color::BLACK,
            ));
        }
        let scene = scene.finish().expect("balanced");
        let (width, height) = device_size(200.0, 100.0, scale_factor);
        let mut target = OffscreenRenderer::new(context, width, height).expect("a target");
        let (_, pixels) = render(&mut target, &scene, scale_factor, &mut text);
        for (index, wanted) in [one, two, thin].into_iter().enumerate() {
            let column = ((35.0 + index as f32 * 60.0) * scale_factor) as u32;
            // The box's top border, and nothing further down.
            let top = first_dark(&pixels, width, column, 0..(15.0 * scale_factor) as u32);
            let run = dark_run(&pixels, width, column, top);
            assert_eq!(
                run, wanted,
                "border {index} at {scale_factor}: {run} device pixels, wanted {wanted}"
            );
            assert!(
                near(pixel(&pixels, width, column, top), [0, 0, 0, 255]),
                "a sharp, opaque line"
            );
            assert!(
                near(
                    pixel(&pixels, width, column, top + run),
                    [255, 255, 255, 255]
                ),
                "white below it"
            );
            assert!(
                near(pixel(&pixels, width, column, top - 1), [255, 255, 255, 255]),
                "white above it"
            );
        }
        for index in 0..3 {
            let column = ((35.0 + index as f32 * 60.0) * scale_factor) as u32;
            let top = first_dark(
                &pixels,
                width,
                column,
                (57.0 * scale_factor) as u32..(85.0 * scale_factor) as u32,
            );
            let run = dark_run(&pixels, width, column, top);
            assert_eq!(run, divider, "divider {index} at {scale_factor}");
        }
    }
}

/// The atlas evicts under pressure and a retained scene still draws the same
/// pixels: one 64-pixel page holds about twelve of these glyphs (no
/// descenders, so one shelf height), frame one
/// draws seven, frame two seven others (evicting some of frame one's), and frame three
/// draws frame one's scene again.
#[test]
fn a_retained_scene_draws_the_same_after_its_glyphs_were_evicted() {
    let Some(context) = software_context() else {
        return;
    };
    let mut text = test_text_system();
    let limits = AtlasLimits {
        page_size: 64,
        maximum_mask_pages: 1,
        maximum_color_pages: 1,
    };
    let mut target =
        OffscreenRenderer::with_atlas_limits(context, 240, 40, limits).expect("a target");
    let scene_of = |text: &mut TextSystem, string: &str| {
        let layout = text.layout(string, &mono(24.0), None);
        let mut scene = SceneBuilder::new();
        scene.text(&layout, Point::new(4.0, 4.0), Color::BLACK);
        scene.finish().expect("balanced")
    };
    let first = scene_of(&mut text, "ABCDEFG");
    let second = scene_of(&mut text, "HIKLMNO");
    let (_, before) = render(&mut target, &first, 1.0, &mut text);
    let (stats, _) = render(&mut target, &second, 1.0, &mut text);
    assert!(
        stats.atlas.evictions > 0,
        "the second frame evicted: {:?}",
        stats.atlas
    );
    assert_eq!(stats.atlas.mask_pages, 1);
    let (stats, after) = render(&mut target, &first, 1.0, &mut text);
    assert_eq!(
        stats.atlas.dropped_glyphs, 0,
        "no frame dropped a glyph: {:?}",
        stats.atlas
    );
    assert!(
        before == after,
        "the retained scene drew different pixels after eviction"
    );
    assert!(
        before.chunks(4).any(|pixel| pixel[0] < 128),
        "the scene drew something"
    );
}

/// What the atlas costs for ordinary text: one 1 MiB mask page.
#[test]
fn ordinary_text_takes_one_mask_page() {
    let Some(context) = software_context() else {
        return;
    };
    let mut text = test_text_system();
    let mut scene = SceneBuilder::new();
    let sizes = [10.0, 12.0, 14.0, 16.0, 20.0, 24.0];
    for (index, size) in sizes.into_iter().enumerate() {
        let layout = text.layout(
            "The quick brown fox jumps over the lazy dog 0123456789",
            &mono(size),
            None,
        );
        scene.text(
            &layout,
            Point::new(4.0, 4.0 + index as f32 * 30.0),
            Color::BLACK,
        );
    }
    let scene = scene.finish().expect("balanced");
    let mut target = OffscreenRenderer::new(context, 900, 200).expect("a target");
    let (stats, _) = render(&mut target, &scene, 1.5, &mut text);
    eprintln!("atlas after six sizes at 150%: {:?}", stats.atlas);
    assert_eq!((stats.atlas.mask_pages, stats.atlas.color_pages), (1, 0));
    assert_eq!(stats.atlas.texture_bytes, 1024 * 1024);
}
