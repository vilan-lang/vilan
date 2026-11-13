//! Measurements of the CPU half of a frame, offscreen, printed as `key=value`
//! lines:
//!
//!     cargo run --release --example measure
//!
//! - `cold_start_ms`: process start to the first offscreen frame read back
//!   (device creation included). Wall time; the GPU is whatever adapter the
//!   system offers (a software one on a machine with no GPU), so this number
//!   says little about a real GPU.
//! - `build_wall_us_median` and `render_wall_us_median`: the time to build a
//!   1,000-primitive scene with `SceneBuilder`, and to encode, upload, record
//!   and submit it (`FrameStats::cpu_time`'s work). Both are CPU-bound on this
//!   thread, so their wall time is their CPU time on an unloaded core; medians
//!   over 50 frames after a warm-up frame, so the atlas is full and no glyph
//!   is rasterised. `thread_cpu_us_per_frame` is this thread's CPU time per
//!   frame over the same loop (build, render and the wait), as a cross-check.
//! - the atlas's pages and bytes after the run.
//! - paths (gfx-path.md S1): `icons_240_cpu_us_median` against
//!   `rectangles_240_cpu_us_median`, the CPU time (`FrameStats::cpu_time`:
//!   encode, upload, record, submit) of a frame of 240 icons at 24 dp whose
//!   masks are cached, and of a frame of 240 rounded rectangles in the same
//!   grid, medians over 50 frames; their ratio; and per icon its parse time
//!   and `first_frame_extra_us`, what a frame drawing it for the first time
//!   costs over the same frame with its mask cached (rasterise and upload),
//!   a median over 21 new device sizes near 24 dp.
//!
//! Thread CPU time is read from `/proc/thread-self/schedstat` (Linux); where
//! that file does not exist the cross-check is not printed.

use std::time::{Duration, Instant};

use vilan_rt_gfx::{
    AdapterPreference, Border, Color, FillRule, GpuContext, LinearGradient, OffscreenRenderer,
    Path, PathPaint, Point, Rect, Rectangle, Scene, SceneBuilder, Shadow, Stroke, TextLayout,
    TextStyle, TextSystem, Underline, Units,
};

#[path = "../tests/support/icons.rs"]
mod icons;

use icons::{ICON_BOX, ICONS, IconPaint, STROKE_WIDTH};

/// This thread's time on a CPU so far.
fn thread_cpu_time() -> Option<Duration> {
    let schedstat = std::fs::read_to_string("/proc/thread-self/schedstat").ok()?;
    let nanoseconds: u64 = schedstat.split_whitespace().next()?.parse().ok()?;
    Some(Duration::from_nanos(nanoseconds))
}

/// 1,000 primitives in a 40 x 25 grid of cells: per cell a rectangle (some
/// with a border or a gradient), and in every other cell a shadow or a text
/// run, with an underline under some of the text. Cells do not overlap, so
/// this is the batching-friendly shape a list or a form has.
fn scene(labels: &[TextLayout]) -> Scene {
    let mut scene = SceneBuilder::new();
    let mut count = 0;
    'cells: for row in 0..25 {
        for column in 0..40 {
            let cell = Rect::new(column as f32 * 48.0, row as f32 * 32.0, 44.0, 28.0);
            let index = row * 40 + column;
            match index % 4 {
                0 => scene.shadow(Shadow {
                    bounds: cell,
                    corner_radius: 4.0,
                    blur: 6.0,
                    color: Color::rgba(0, 0, 0, 60),
                }),
                1 => scene.rectangle(
                    Rectangle::new(
                        cell,
                        LinearGradient {
                            start: cell.origin,
                            end: Point::new(cell.right(), cell.top()),
                            start_color: Color::rgb(250, 120, 0),
                            end_color: Color::rgb(40, 90, 250),
                        },
                    )
                    .with_radius(6.0),
                ),
                2 => scene.rectangle(
                    Rectangle::new(cell, Color::WHITE)
                        .with_radius(4.0)
                        .with_border(Border::all(1.0, Color::rgb(180, 180, 190))),
                ),
                _ => {
                    let label = &labels[index % labels.len()];
                    scene.text(
                        label,
                        Point::new(cell.left() + 2.0, cell.top() + 4.0),
                        Color::BLACK,
                    );
                    if index % 8 == 3 {
                        scene.underline(Underline {
                            origin: Point::new(cell.left() + 2.0, cell.top() + 22.0),
                            width: 30.0,
                            thickness: 1.0,
                            color: Color::rgb(200, 40, 40),
                        });
                        count += 1;
                    }
                }
            }
            count += 1;
            if count >= 1000 {
                break 'cells;
            }
        }
    }
    scene.finish().expect("a balanced scene")
}

fn icon_paint(paint: IconPaint) -> PathPaint {
    match paint {
        IconPaint::Stroke => PathPaint::Stroke(Stroke::new(STROKE_WIDTH)),
        IconPaint::Fill => PathPaint::Fill(FillRule::NonZero),
        IconPaint::FillEvenOdd => PathPaint::Fill(FillRule::EvenOdd),
    }
}

/// 240 cells of 32 dp in a 24 x 10 grid: an icon of 24 dp in each (the
/// seven in turn), or a rounded rectangle of 24 dp.
fn grid(paths: &[(Path, PathPaint)], icons: bool) -> Scene {
    let mut scene = SceneBuilder::new();
    for index in 0..240 {
        let (x, y) = ((index % 24) as f32 * 32.0, (index / 24) as f32 * 32.0);
        if icons {
            let (path, paint) = &paths[index % paths.len()];
            scene.path(
                path,
                Point::new(x, y),
                24.0 / ICON_BOX,
                *paint,
                Color::BLACK,
            );
        } else {
            scene.rectangle(
                Rectangle::new(Rect::new(x, y, 24.0, 24.0), Color::BLACK).with_radius(6.0),
            );
        }
    }
    scene.finish().expect("a balanced scene")
}

/// The median CPU time of `frames` renders of `scene` (after one warm-up).
fn render_cpu(
    target: &mut OffscreenRenderer,
    scene: &Scene,
    text: &mut TextSystem,
    frames: usize,
) -> Duration {
    let units = Units::new(1.0, 1.0).expect("valid units");
    target
        .render(scene, units, text, Color::WHITE)
        .expect("warm-up");
    target.finish().expect("the GPU finishes");
    let mut times = Vec::new();
    for _ in 0..frames {
        let stats = target
            .render(scene, units, text, Color::WHITE)
            .expect("a frame");
        target.finish().expect("the GPU finishes");
        times.push(stats.cpu_time);
    }
    median(times)
}

fn measure_paths(context: GpuContext, text: &mut TextSystem) {
    let paths: Vec<(Path, PathPaint)> = ICONS
        .iter()
        .map(|icon| {
            (
                Path::from_svg(icon.data).expect("the icons parse"),
                icon_paint(icon.paint),
            )
        })
        .collect();
    let mut target = OffscreenRenderer::new(context, 768, 320).expect("a target");
    let icons = render_cpu(&mut target, &grid(&paths, true), text, 50);
    let rectangles = render_cpu(&mut target, &grid(&paths, false), text, 50);
    println!("icons_240_cpu_us_median={}", icons.as_micros());
    println!("rectangles_240_cpu_us_median={}", rectangles.as_micros());
    println!(
        "icons_over_rectangles={:.3}",
        icons.as_secs_f64() / rectangles.as_secs_f64()
    );

    let units = Units::new(1.0, 1.0).expect("valid units");
    for (icon, (path, paint)) in ICONS.iter().zip(&paths) {
        let mut parses = Vec::new();
        for _ in 0..21 {
            let started = Instant::now();
            std::hint::black_box(Path::from_svg(icon.data).expect("parses"));
            parses.push(started.elapsed());
        }
        let mut extra = Vec::new();
        for step in 0..21 {
            // A device size no frame drew before: a new mask.
            let scale = 24.0 / ICON_BOX * (1.0 + (step + 1) as f32 * 1.0e-3);
            let mut scene = SceneBuilder::new();
            scene.path(path, Point::new(4.0, 4.0), scale, *paint, Color::BLACK);
            let scene = scene.finish().expect("balanced");
            let first = target
                .render(&scene, units, text, Color::WHITE)
                .expect("a frame");
            target.finish().expect("the GPU finishes");
            let cached = target
                .render(&scene, units, text, Color::WHITE)
                .expect("a frame");
            target.finish().expect("the GPU finishes");
            extra.push(first.cpu_time.saturating_sub(cached.cpu_time));
        }
        println!(
            "icon={:?} parse_us={:.1} first_frame_extra_us={:.1}",
            icon.name,
            median(parses).as_secs_f64() * 1.0e6,
            median(extra).as_secs_f64() * 1.0e6
        );
    }
}

fn median(mut values: Vec<Duration>) -> Duration {
    values.sort();
    values[values.len() / 2]
}

fn main() {
    let process_started = Instant::now();
    let context = match GpuContext::headless(AdapterPreference::HighPerformance) {
        Ok(context) => context,
        Err(error) => {
            println!("failure: {error}");
            std::process::exit(1);
        }
    };
    let adapter = context.adapter();
    println!(
        "adapter={} backend={} type={}",
        adapter.name, adapter.backend, adapter.device_type
    );
    let units = Units::new(1.0, 1.0).expect("valid units");
    let mut text = TextSystem::new();
    let mut target = OffscreenRenderer::new(context, 1920, 800).expect("a target");

    let labels: Vec<TextLayout> = (0..50)
        .map(|index| text.layout(&format!("item {index}"), &TextStyle::default(), None))
        .collect();
    let first = scene(&labels);
    target
        .render(&first, units, &mut text, Color::WHITE)
        .expect("the first frame");
    target.read_rgba().expect("read back");
    println!(
        "cold_start_ms={:.1}",
        process_started.elapsed().as_secs_f64() * 1000.0
    );
    println!("primitives={}", first.primitive_count());

    // Build and render are CPU-bound on this thread with no blocking inside,
    // so each is timed by the wall clock (median of 50); the thread's CPU
    // time over the whole loop (which blocks once per frame, in the read-back,
    // so the kernel's count is current) cross-checks the sum.
    let mut build_times = Vec::new();
    let mut render_times = Vec::new();
    let mut stats = None;
    let frames = 50;
    let cpu_before = thread_cpu_time();
    for _ in 0..frames {
        let started = Instant::now();
        let frame_scene = scene(&labels);
        build_times.push(started.elapsed());
        let started = Instant::now();
        let frame_stats = target
            .render(&frame_scene, units, &mut text, Color::WHITE)
            .expect("a frame");
        render_times.push(started.elapsed());
        // Wait for the GPU so frames do not pile up in the queue.
        target.finish().expect("the GPU finishes");
        stats = Some(frame_stats);
    }
    let cpu_after = thread_cpu_time();
    println!("build_wall_us_median={}", median(build_times).as_micros());
    println!("render_wall_us_median={}", median(render_times).as_micros());
    if let (Some(before), Some(after)) = (cpu_before, cpu_after) {
        println!(
            "thread_cpu_us_per_frame={}",
            ((after - before) / frames).as_micros()
        );
    }
    let stats = stats.expect("fifty frames");
    println!(
        "draw_calls={} quads={} sprites={} culled={}",
        stats.draw_calls, stats.quad_instances, stats.sprite_instances, stats.culled_primitives
    );
    println!(
        "atlas_mask_pages={} atlas_color_pages={} atlas_bytes={} resident_glyphs={}",
        stats.atlas.mask_pages,
        stats.atlas.color_pages,
        stats.atlas.texture_bytes,
        stats.atlas.resident_glyphs
    );
    let context = target.context().clone();
    measure_paths(context, &mut text);
}
