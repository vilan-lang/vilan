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
//!
//! Thread CPU time is read from `/proc/thread-self/schedstat` (Linux); where
//! that file does not exist the cross-check is not printed.

use std::time::{Duration, Instant};

use vilan_rt_gfx::{
    AdapterPreference, Border, Color, GpuContext, LinearGradient, OffscreenRenderer, Point, Rect,
    Rectangle, Scene, SceneBuilder, Shadow, TextLayout, TextStyle, TextSystem, Underline, Units,
};

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
}
