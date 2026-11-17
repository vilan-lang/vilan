//! `VILAN_GFX_REPORT=1`: a plain-text account of what a `std::gfx` program
//! ran on and what its frames cost, printed to STDERR so it can be pasted back
//! from a machine nobody here can see (F187: the first runs on real GPUs, on
//! Windows and on macOS).
//!
//! # What it prints
//!
//! - Once per window, when its adapter, device and surface exist: the build,
//!   the system, the adapter (wgpu's `AdapterInfo`), the texture limit, the
//!   surface's format, present mode and alpha mode (with what the surface
//!   supports), the window's size and scale factor, and its monitor.
//! - While it runs, one line each: a scale factor change; a surface the
//!   window system declared suboptimal, outdated or lost, a frame skipped for
//!   a timeout or an occluded window; and a resize — at most one line per
//!   quarter second, since a drag resizes at the display's rate (the exit
//!   report counts every one).
//! - When the window system's loop ends (the last window closed, SIGTERM, a
//!   failure): frames presented and skipped, and for each part of a frame the
//!   count, median, 95th percentile and maximum in microseconds — see
//!   [`Report::exit_text`] for the parts and the honest rule for the
//!   interval between frames — then the glyph atlas and the instance buffers.
//!
//! # What it costs when it is off
//!
//! Nothing that runs per primitive. [`install`] reads the variable ONCE, on
//! the main thread, and leaves [`REPORT`] empty when it is unset; every hook
//! is then a thread-local read and a branch, and takes no clock reading of
//! its own (the times it records are ones the host and the renderer already
//! take for `ui::last_frame()` and `FrameStats`).
//!
//! The process ends without the exit report only when it is stopped at once
//! (a second termination request, or a kill the program cannot see).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::time::{Duration, Instant};

use winit::window::Window;

use crate::atlas::AtlasStats;
use crate::renderer::FrameStats;
use crate::window::{SurfaceCounts, WindowRenderer};

/// The environment variable that turns the report on: any value but `0` (or
/// empty).
pub(crate) const VARIABLE: &str = "VILAN_GFX_REPORT";

/// Samples kept per measure; the count and the maximum cover every frame,
/// the median and the 95th percentile the first this many.
pub(crate) const SAMPLE_CAP: usize = 100_000;

/// A request made at most this long after the previous present still counts
/// as asking for the next frame back to back.
pub(crate) const BACK_TO_BACK_SLACK: Duration = Duration::from_millis(1);

/// At most one resize line per this long.
const RESIZE_LINE_EVERY: Duration = Duration::from_millis(250);

/// Every line printed while the program runs starts with this.
const PREFIX: &str = "vilan gfx report: ";

thread_local! {
    /// The report being collected, when [`VARIABLE`] asked for one.
    static REPORT: RefCell<Option<Report>> = const { RefCell::new(None) };
}

/// Whether `value` (the variable's) turns the report on.
pub(crate) fn wanted(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty() && value != "0")
}

/// Reads [`VARIABLE`] once: called by the host's `install`, on the main
/// thread. `unavailable` is why the window system did not start, if it did
/// not (the report then says so and nothing else).
pub(crate) fn install(unavailable: Option<&str>) {
    if !wanted(std::env::var_os(VARIABLE).as_deref()) {
        return;
    }
    if let Some(reason) = unavailable {
        emit(&format!("{PREFIX}no window system: {reason}\n"));
    }
    REPORT.with(|report| *report.borrow_mut() = Some(Report::new(Instant::now())));
}

/// Runs `body` on the report, when one is being collected.
fn with_report(body: impl FnOnce(&mut Report)) {
    REPORT.with(|report| {
        if let Some(report) = report.borrow_mut().as_mut() {
            body(report);
        }
    });
}

/// Writes `text` to stderr in one piece. A closed stderr loses the report,
/// never the program.
fn emit(text: &str) {
    let _ = std::io::stderr().lock().write_all(text.as_bytes());
}

/// Window `id` exists with its renderer: the start report.
pub(crate) fn window_ready(id: u32, renderer: &WindowRenderer) {
    with_report(|report| {
        report.windows.entry(id).or_default();
        emit(&StartFacts::read(id, renderer).text());
    });
}

/// A frame of window `id` was asked for now (by the program, the system, a
/// resize or a scale change).
pub(crate) fn requested(id: u32) {
    with_report(|report| {
        let window = report.windows.entry(id).or_default();
        window.pending.get_or_insert_with(Instant::now);
    });
}

/// Window `id`'s frame callback built its scene in `took`: the requests made
/// so far are this frame's.
pub(crate) fn frame_built(id: u32, took: Duration) {
    with_report(|report| {
        report.frame_build.push(took);
        let window = report.windows.entry(id).or_default();
        if let Some(pending) = window.pending.take() {
            window.served = Some(window.served.map_or(pending, |served| served.min(pending)));
        }
    });
}

/// Whether a report is being collected: the host times a frame's build only
/// then.
pub(crate) fn active() -> bool {
    REPORT.with(|report| report.borrow().is_some())
}

/// A `std::ui` layout took `micros`.
pub(crate) fn layout(micros: f64) {
    with_report(|report| report.layout.push_micros(micros));
}

/// A `std::ui` paint took `micros`.
pub(crate) fn paint(micros: f64) {
    with_report(|report| report.paint.push_micros(micros));
}

/// Window `id` presented a frame, finishing at `presented`.
pub(crate) fn presented(id: u32, stats: &FrameStats, presented: Instant) {
    with_report(|report| report.presented(id, stats, presented));
}

/// Window `id` skipped a frame (`FrameOutcome::Skipped`).
pub(crate) fn skipped() {
    with_report(|report| report.frames_skipped += 1);
}

/// What the surface of window `id` said while acquiring a frame: one line
/// per kind that happened since `before`.
pub(crate) fn surface(id: u32, before: SurfaceCounts, after: SurfaceCounts, size: (u32, u32)) {
    with_report(|_| {
        let mut text = String::new();
        let (width, height) = size;
        for (kind, was, now, what) in [
            (
                "suboptimal",
                before.suboptimal,
                after.suboptimal,
                "drawn, configured again",
            ),
            (
                "outdated",
                before.outdated,
                after.outdated,
                "skipped, configured again",
            ),
            (
                "lost",
                before.lost,
                after.lost,
                "skipped, surface made again",
            ),
            ("timeout", before.timeout, after.timeout, "skipped"),
            (
                "occluded",
                before.occluded,
                after.occluded,
                "skipped: minimised or covered",
            ),
        ] {
            if now > was {
                let _ = writeln!(
                    text,
                    "{PREFIX}window {id} surface: {kind} ({what}) at {width}x{height} px, {now} so far"
                );
            }
        }
        emit(&text);
    });
}

/// Window `id`'s surface follows a new size, in device pixels.
pub(crate) fn resized(id: u32, width: u32, height: u32) {
    with_report(|report| {
        report.resizes += 1;
        let now = Instant::now();
        let due = report
            .resize_line_at
            .is_none_or(|last| now.duration_since(last) >= RESIZE_LINE_EVERY);
        if due {
            report.resize_line_at = Some(now);
            emit(&format!(
                "{PREFIX}window {id} surface: resized to {width}x{height} px (resize {})\n",
                report.resizes
            ));
        }
    });
}

/// Window `id`'s scale factor became `scale_factor`.
pub(crate) fn scale_changed(id: u32, scale_factor: f64, window: &Window) {
    with_report(|report| {
        report.scale_changes += 1;
        let size = window.inner_size();
        emit(&format!(
            "{PREFIX}window {id} scale factor: {scale_factor} (inner size {}x{} px; monitor {})\n",
            size.width,
            size.height,
            MonitorFacts::read(window)
                .map_or_else(|| "unknown".to_string(), |monitor| monitor.text())
        ));
    });
}

/// The window system's loop has ended: the exit report, once.
pub(crate) fn finish() {
    let report = REPORT.with(|report| report.borrow_mut().take());
    if let Some(report) = report {
        emit(&report.exit_text(report.started.elapsed()));
    }
}

// -------------------------------------------------------------- samples ---

/// One measure's samples, in whole microseconds.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Samples {
    /// The first [`SAMPLE_CAP`] samples.
    kept: Vec<u32>,
    /// Every sample, kept or not.
    count: u64,
    max: u32,
}

/// What [`Samples::summary`] answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Summary {
    pub(crate) count: u64,
    pub(crate) median: u32,
    pub(crate) p95: u32,
    pub(crate) max: u32,
    /// How many samples the median and the 95th percentile were read from.
    pub(crate) kept: usize,
}

impl Samples {
    pub(crate) fn push(&mut self, took: Duration) {
        self.push_value(u32::try_from(took.as_micros()).unwrap_or(u32::MAX));
    }

    /// A time already in (fractional) microseconds, rounded to the nearest.
    pub(crate) fn push_micros(&mut self, micros: f64) {
        // `as` saturates, and a NaN becomes 0.
        self.push_value(micros.round() as u32);
    }

    fn push_value(&mut self, value: u32) {
        self.count += 1;
        self.max = self.max.max(value);
        if self.kept.len() < SAMPLE_CAP {
            self.kept.push(value);
        }
    }

    /// Count, median, 95th percentile and maximum; `None` with no sample.
    /// Percentiles are NEAREST-RANK over the kept samples: the smallest
    /// sample with at least that share of the samples at or below it (so the
    /// median of an even count is the lower middle one, and every answer is a
    /// time some frame really took).
    pub(crate) fn summary(&self) -> Option<Summary> {
        if self.kept.is_empty() {
            return None;
        }
        let mut sorted = self.kept.clone();
        sorted.sort_unstable();
        Some(Summary {
            count: self.count,
            median: nearest_rank(&sorted, 50),
            p95: nearest_rank(&sorted, 95),
            max: self.max,
            kept: sorted.len(),
        })
    }
}

/// The `percent`th percentile of `sorted` (not empty), nearest-rank.
fn nearest_rank(sorted: &[u32], percent: usize) -> u32 {
    let rank = (percent * sorted.len()).div_ceil(100).max(1);
    sorted[rank - 1]
}

// --------------------------------------------------------------- report ---

/// A window's frame requests, for the interval rule.
#[derive(Clone, Copy, Debug, Default)]
struct WindowTimes {
    /// The earliest request not yet taken by a frame build.
    pending: Option<Instant>,
    /// The earliest request the built (and not yet presented) frame serves.
    served: Option<Instant>,
    /// When the window's last frame finished presenting.
    last_present: Option<Instant>,
}

/// Everything the exit report sums up.
#[derive(Debug)]
pub(crate) struct Report {
    started: Instant,
    windows: BTreeMap<u32, WindowTimes>,
    frames_presented: u64,
    frames_skipped: u64,
    layout: Samples,
    paint: Samples,
    frame_build: Samples,
    encode: Samples,
    submit: Samples,
    acquire: Samples,
    present: Samples,
    interval: Samples,
    /// Presents that had a previous present of their window: the intervals
    /// there were, back to back or not.
    intervals_seen: u64,
    resizes: u64,
    resize_line_at: Option<Instant>,
    scale_changes: u64,
    atlas: Option<AtlasStats>,
    peak_atlas_bytes: u64,
    peak_path_mask_textures: u32,
    peak_quads: u32,
    peak_sprites: u32,
    peak_draw_calls: u32,
    peak_primitives: u32,
    peak_instance_bytes: u64,
}

impl Report {
    pub(crate) fn new(started: Instant) -> Report {
        Report {
            started,
            windows: BTreeMap::new(),
            frames_presented: 0,
            frames_skipped: 0,
            layout: Samples::default(),
            paint: Samples::default(),
            frame_build: Samples::default(),
            encode: Samples::default(),
            submit: Samples::default(),
            acquire: Samples::default(),
            present: Samples::default(),
            interval: Samples::default(),
            intervals_seen: 0,
            resizes: 0,
            resize_line_at: None,
            scale_changes: 0,
            atlas: None,
            peak_atlas_bytes: 0,
            peak_path_mask_textures: 0,
            peak_quads: 0,
            peak_sprites: 0,
            peak_draw_calls: 0,
            peak_primitives: 0,
            peak_instance_bytes: 0,
        }
    }

    /// A frame of window `id` finished presenting at `presented`.
    ///
    /// THE INTERVAL RULE. Frames are drawn on demand, so the time between two
    /// presents is mostly the time until somebody asked for the second one.
    /// It is counted only when the second frame was asked for no later than
    /// [`BACK_TO_BACK_SLACK`] after the first finished presenting — the
    /// earliest request a frame serves is known exactly (a program's
    /// `request_redraw`, the system's redraw, a resize, a scale change) — so
    /// what it measures is how fast frames CAN follow one another: the whole
    /// frame's cost plus any wait for the display (vsync).
    pub(crate) fn presented(&mut self, id: u32, stats: &FrameStats, presented: Instant) {
        self.frames_presented += 1;
        self.encode.push(stats.encode_time);
        self.submit
            .push(stats.cpu_time.saturating_sub(stats.encode_time));
        self.acquire.push(stats.acquire_time);
        self.present.push(stats.present_time);
        let window = self.windows.entry(id).or_default();
        let served = window.served.take();
        if let Some(previous) = window.last_present {
            self.intervals_seen += 1;
            if served.is_some_and(|asked| asked <= previous + BACK_TO_BACK_SLACK) {
                self.interval
                    .push(presented.saturating_duration_since(previous));
            }
        }
        window.last_present = Some(presented);
        self.atlas = Some(stats.atlas);
        self.peak_atlas_bytes = self.peak_atlas_bytes.max(stats.atlas.texture_bytes);
        self.peak_path_mask_textures = self.peak_path_mask_textures.max(stats.path_mask_textures);
        self.peak_quads = self.peak_quads.max(stats.quad_instances);
        self.peak_sprites = self.peak_sprites.max(stats.sprite_instances);
        self.peak_draw_calls = self.peak_draw_calls.max(stats.draw_calls);
        self.peak_primitives = self.peak_primitives.max(stats.primitives);
        self.peak_instance_bytes = self.peak_instance_bytes.max(stats.instance_buffer_bytes);
    }

    /// The exit report, after `run_time` of running.
    pub(crate) fn exit_text(&self, run_time: Duration) -> String {
        let mut text = String::new();
        let mut line = |key: &str, value: String| {
            let _ = writeln!(text, "{key}: {value}");
        };
        line("--- vilan gfx report", "exit ---".to_string());
        line("run time", format!("{:.1} s", run_time.as_secs_f64()));
        line("frames presented", self.frames_presented.to_string());
        line(
            "frames skipped",
            format!(
                "{} (minimised, covered, timed out, or the surface had to be made again)",
                self.frames_skipped
            ),
        );
        line("surface resizes", self.resizes.to_string());
        line("scale factor changes", self.scale_changes.to_string());
        line(
            "times",
            "microseconds elapsed on the main thread (wall time, not CPU time), per frame: \
             count, median, 95th percentile, max"
                .to_string(),
        );
        for (key, samples, what) in [
            ("layout", &self.layout, "std::ui layout"),
            ("paint", &self.paint, "std::ui paint, into the scene"),
            (
                "frame build",
                &self.frame_build,
                "the frame callback: std::ui's layout and paint, or the program's drawing",
            ),
            (
                "encode",
                &self.encode,
                "scene to instances, glyphs and path masks rasterised",
            ),
            ("submit", &self.submit, "upload, record and submit"),
            (
                "acquire",
                &self.acquire,
                "waiting for the surface's next texture",
            ),
            ("present", &self.present, "handing the frame to the system"),
        ] {
            line(key, summary_text(samples, what));
        }
        line(
            "interval",
            format!(
                "{}; counted only between frames asked for back to back (the next frame \
                 requested before, or within {} ms after, the previous present): {} of {} \
                 intervals",
                summary_text(&self.interval, "between presents"),
                BACK_TO_BACK_SLACK.as_millis(),
                self.interval.count,
                self.intervals_seen
            ),
        );
        match &self.atlas {
            Some(atlas) => line(
                "glyph atlas",
                format!(
                    "{} mask pages, {} colour pages, {} (peak {}), {} glyphs resident, {} \
                     evicted, {} dropped",
                    atlas.mask_pages,
                    atlas.color_pages,
                    bytes_text(atlas.texture_bytes),
                    bytes_text(self.peak_atlas_bytes),
                    atlas.resident_glyphs,
                    atlas.evictions,
                    atlas.dropped_glyphs
                ),
            ),
            None => line("glyph atlas", "no frame presented".to_string()),
        }
        line(
            "path mask textures",
            format!("{} at most", self.peak_path_mask_textures),
        );
        line(
            "peak per frame",
            format!(
                "{} primitives, {} quad instances, {} sprite instances, {} draw calls",
                self.peak_primitives, self.peak_quads, self.peak_sprites, self.peak_draw_calls
            ),
        );
        line(
            "instance buffers",
            format!("{} at most", bytes_text(self.peak_instance_bytes)),
        );
        text.push_str("--- end ---\n");
        text
    }
}

/// `count N, median M us, p95 P us, max X us (what)`.
fn summary_text(samples: &Samples, what: &str) -> String {
    match samples.summary() {
        None => format!("count 0 ({what})"),
        Some(summary) => {
            let kept = if summary.count > summary.kept as u64 {
                format!(", percentiles of the first {}", summary.kept)
            } else {
                String::new()
            };
            format!(
                "count {}, median {} us, p95 {} us, max {} us ({what}{kept})",
                summary.count, summary.median, summary.p95, summary.max
            )
        }
    }
}

/// A byte count in KiB or MiB.
fn bytes_text(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    }
}

// ---------------------------------------------------------------- start ---

/// A window's monitor, as `winit` describes it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MonitorFacts {
    pub(crate) name: Option<String>,
    pub(crate) refresh_millihertz: Option<u32>,
    pub(crate) size: (u32, u32),
    pub(crate) scale_factor: f64,
}

impl MonitorFacts {
    fn read(window: &Window) -> Option<MonitorFacts> {
        let monitor = window.current_monitor()?;
        let size = monitor.size();
        Some(MonitorFacts {
            name: monitor.name(),
            refresh_millihertz: monitor.refresh_rate_millihertz(),
            size: (size.width, size.height),
            scale_factor: monitor.scale_factor(),
        })
    }

    fn text(&self) -> String {
        let refresh = self.refresh_millihertz.map_or_else(
            || "refresh unknown".to_string(),
            |millihertz| format!("{:.3} Hz", f64::from(millihertz) / 1000.0),
        );
        format!(
            "{}, {}x{} px, {refresh}, scale factor {}",
            self.name.as_deref().unwrap_or("(no name)"),
            self.size.0,
            self.size.1,
            self.scale_factor
        )
    }
}

/// What the start report says about one window.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StartFacts {
    pub(crate) window: u32,
    pub(crate) crate_version: &'static str,
    pub(crate) profile: &'static str,
    pub(crate) os: &'static str,
    pub(crate) arch: &'static str,
    pub(crate) wgpu_backend: Option<String>,
    pub(crate) wgpu_debug: Option<String>,
    pub(crate) adapter: wgpu::AdapterInfo,
    pub(crate) adapter_max_texture: u32,
    pub(crate) device_max_texture: u32,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) formats: Vec<wgpu::TextureFormat>,
    pub(crate) present_mode: wgpu::PresentMode,
    pub(crate) present_modes: Vec<wgpu::PresentMode>,
    pub(crate) alpha_mode: wgpu::CompositeAlphaMode,
    pub(crate) alpha_modes: Vec<wgpu::CompositeAlphaMode>,
    pub(crate) frame_latency: u32,
    pub(crate) surface_size: (u32, u32),
    pub(crate) inner_size: (u32, u32),
    pub(crate) scale_factor: f64,
    pub(crate) text_scale: f64,
    pub(crate) monitor: Option<MonitorFacts>,
}

impl StartFacts {
    fn read(window: u32, renderer: &WindowRenderer) -> StartFacts {
        let context = renderer.context();
        let configuration = renderer.configuration();
        let capabilities = renderer.capabilities();
        let handle = renderer.window();
        let inner = handle.inner_size();
        StartFacts {
            window,
            crate_version: env!("CARGO_PKG_VERSION"),
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            wgpu_backend: std::env::var("WGPU_BACKEND").ok(),
            wgpu_debug: std::env::var("WGPU_DEBUG").ok(),
            adapter: context.adapter.get_info(),
            adapter_max_texture: context.adapter.limits().max_texture_dimension_2d,
            device_max_texture: context.maximum_texture_side(),
            format: configuration.format,
            formats: capabilities.formats,
            present_mode: configuration.present_mode,
            present_modes: capabilities.present_modes,
            alpha_mode: configuration.alpha_mode,
            alpha_modes: capabilities.alpha_modes,
            frame_latency: configuration.desired_maximum_frame_latency,
            surface_size: (configuration.width, configuration.height),
            inner_size: (inner.width, inner.height),
            scale_factor: handle.scale_factor(),
            text_scale: super::TEXT_SCALE,
            monitor: MonitorFacts::read(handle),
        }
    }

    pub(crate) fn text(&self) -> String {
        let mut text = String::new();
        let mut line = |key: &str, value: String| {
            let _ = writeln!(text, "{key}: {value}");
        };
        let list = |items: Vec<String>| items.join(", ");
        let adapter = &self.adapter;
        line("--- vilan gfx report", "start ---".to_string());
        line("window", self.window.to_string());
        line(
            "crate",
            format!(
                "vilan-rt-gfx {} ({} build)",
                self.crate_version, self.profile
            ),
        );
        line("os", format!("{} {}", self.os, self.arch));
        line(
            "WGPU_BACKEND",
            self.wgpu_backend
                .clone()
                .unwrap_or_else(|| "(unset)".to_string()),
        );
        line(
            "WGPU_DEBUG",
            self.wgpu_debug
                .clone()
                .unwrap_or_else(|| "(unset)".to_string()),
        );
        line("adapter", adapter.name.clone());
        line(
            "adapter vendor",
            format!("{:#06x}{}", adapter.vendor, vendor_name(adapter.vendor)),
        );
        line("adapter device", format!("{:#06x}", adapter.device));
        line(
            "adapter type",
            match adapter.device_type {
                wgpu::DeviceType::DiscreteGpu => "discrete GPU",
                wgpu::DeviceType::IntegratedGpu => "integrated GPU",
                wgpu::DeviceType::VirtualGpu => "virtual GPU",
                wgpu::DeviceType::Cpu => "CPU (a software renderer)",
                wgpu::DeviceType::Other => "other",
            }
            .to_string(),
        );
        line("backend", format!("{:?}", adapter.backend));
        let or_none = |value: &str| {
            if value.is_empty() {
                "(none given)".to_string()
            } else {
                value.to_string()
            }
        };
        line("driver", or_none(&adapter.driver));
        line("driver info", or_none(&adapter.driver_info));
        line(
            "max texture side",
            format!(
                "{} (adapter {}; the device asks WebGL2's limits, raised to the adapter's \
                 texture size)",
                self.device_max_texture, self.adapter_max_texture
            ),
        );
        line(
            "surface format",
            format!(
                "{:?} (supported: {})",
                self.format,
                list(
                    self.formats
                        .iter()
                        .map(|format| format!("{format:?}"))
                        .collect()
                )
            ),
        );
        line(
            "present mode",
            format!(
                "{:?} (supported: {})",
                self.present_mode,
                list(
                    self.present_modes
                        .iter()
                        .map(|mode| format!("{mode:?}"))
                        .collect()
                )
            ),
        );
        line(
            "alpha mode",
            format!(
                "{:?} (supported: {})",
                self.alpha_mode,
                list(
                    self.alpha_modes
                        .iter()
                        .map(|mode| format!("{mode:?}"))
                        .collect()
                )
            ),
        );
        line(
            "frame latency",
            format!("{} frames at most", self.frame_latency),
        );
        line(
            "surface size",
            format!("{}x{} px", self.surface_size.0, self.surface_size.1),
        );
        line(
            "window inner size",
            format!("{}x{} px", self.inner_size.0, self.inner_size.1),
        );
        line("scale factor", self.scale_factor.to_string());
        line(
            "text scale",
            format!(
                "{} (the system's text size setting is not read)",
                self.text_scale
            ),
        );
        line(
            "monitor",
            self.monitor
                .as_ref()
                .map_or_else(|| "unknown".to_string(), MonitorFacts::text),
        );
        text.push_str("--- end ---\n");
        text
    }
}

/// The PCI vendor ids of the GPU makers a desktop meets, and Mesa's software
/// drivers' id, as ` (name)`; nothing for another id.
fn vendor_name(vendor: u32) -> &'static str {
    match vendor {
        0x1002 | 0x1022 => " (AMD)",
        0x10de => " (NVIDIA)",
        0x8086 => " (Intel)",
        0x106b => " (Apple)",
        0x5143 => " (Qualcomm)",
        0x13b5 => " (Arm)",
        0x1414 => " (Microsoft)",
        0x10005 => " (Mesa)",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(values: &[u32]) -> Samples {
        let mut samples = Samples::default();
        for value in values {
            samples.push(Duration::from_micros(u64::from(*value)));
        }
        samples
    }

    #[test]
    fn the_report_is_on_for_any_value_but_empty_and_zero() {
        assert!(!wanted(None));
        assert!(!wanted(Some("".as_ref())));
        assert!(!wanted(Some("0".as_ref())));
        assert!(wanted(Some("1".as_ref())));
        assert!(wanted(Some("yes".as_ref())));
    }

    #[test]
    fn percentiles_are_nearest_rank_over_the_samples() {
        assert_eq!(Samples::default().summary(), None);
        let one = samples(&[7]).summary().expect("one sample");
        assert_eq!((one.count, one.median, one.p95, one.max), (1, 7, 7, 7));
        // An even count: the lower middle sample, never an average.
        let four = samples(&[40, 10, 30, 20]).summary().expect("four");
        assert_eq!((four.median, four.p95, four.max), (20, 40, 40));
        // 1..=100 in any order: the 50th and the 95th.
        let mut hundred: Vec<u32> = (1..=100).rev().collect();
        hundred.swap(3, 70);
        let summary = samples(&hundred).summary().expect("a hundred");
        assert_eq!(
            (summary.count, summary.median, summary.p95, summary.max),
            (100, 50, 95, 100)
        );
        // Twenty: rank ceil(0.95 * 20) = 19.
        let twenty: Vec<u32> = (1..=20).collect();
        assert_eq!(samples(&twenty).summary().expect("twenty").p95, 19);
    }

    #[test]
    fn past_the_cap_the_count_and_maximum_go_on_and_the_percentiles_keep_the_first() {
        let mut samples = Samples::default();
        for _ in 0..SAMPLE_CAP {
            samples.push(Duration::from_micros(5));
        }
        samples.push(Duration::from_micros(900));
        let summary = samples.summary().expect("samples");
        assert_eq!(summary.count, SAMPLE_CAP as u64 + 1);
        assert_eq!(summary.kept, SAMPLE_CAP);
        assert_eq!((summary.median, summary.p95, summary.max), (5, 5, 900));
        assert!(summary_text(&samples, "x").ends_with("(x, percentiles of the first 100000)"));
    }

    #[test]
    fn fractional_microseconds_round_and_nonsense_saturates() {
        let mut samples = Samples::default();
        samples.push_micros(1.4);
        samples.push_micros(1.6);
        samples.push_micros(f64::NAN);
        samples.push_micros(1.0e12);
        assert_eq!(samples.kept, vec![1, 2, 0, u32::MAX]);
    }

    fn frame(encode: u64, cpu: u64, acquire: u64, present: u64) -> FrameStats {
        FrameStats {
            draw_calls: 3,
            quad_instances: 40,
            sprite_instances: 120,
            primitives: 60,
            culled_primitives: 0,
            atlas: AtlasStats {
                mask_pages: 1,
                color_pages: 0,
                texture_bytes: 4 * 1024 * 1024,
                resident_glyphs: 57,
                evictions: 0,
                dropped_glyphs: 0,
            },
            path_mask_textures: 0,
            cpu_time: Duration::from_micros(cpu),
            encode_time: Duration::from_micros(encode),
            instance_buffer_bytes: 98_304,
            acquire_time: Duration::from_micros(acquire),
            present_time: Duration::from_micros(present),
        }
    }

    /// Three frames of one window: the second asked for while the first was
    /// being built (back to back), the third long after the second's present
    /// (an idle gap, not counted).
    #[test]
    fn the_exit_report_from_fixed_frames() {
        let start = Instant::now();
        let at = |micros: u64| start + Duration::from_micros(micros);
        let mut report = Report::new(start);
        report.layout.push_micros(120.4);
        report.layout.push_micros(80.0);
        report.paint.push_micros(300.0);
        report.paint.push_micros(30.0);
        report.frame_build.push(Duration::from_micros(450));
        report.frame_build.push(Duration::from_micros(130));
        report.frame_build.push(Duration::from_micros(90));
        // Frame 1: no earlier present.
        report.windows.entry(1).or_default().served = Some(at(0));
        report.presented(1, &frame(200, 500, 10, 40), at(10_000));
        // Frame 2: asked for at 9 ms, before frame 1's present ended.
        report.windows.entry(1).or_default().served = Some(at(9_000));
        report.presented(1, &frame(100, 300, 16_000, 30), at(26_667));
        // Frame 3: asked for 2 s after frame 2's present.
        report.windows.entry(1).or_default().served = Some(at(2_026_667));
        report.presented(1, &frame(90, 250, 12, 25), at(2_030_000));
        report.frames_skipped = 1;
        report.resizes = 4;
        assert_eq!(
            report.exit_text(Duration::from_millis(2_500)),
            "\
--- vilan gfx report: exit ---
run time: 2.5 s
frames presented: 3
frames skipped: 1 (minimised, covered, timed out, or the surface had to be made again)
surface resizes: 4
scale factor changes: 0
times: microseconds elapsed on the main thread (wall time, not CPU time), per frame: count, median, 95th percentile, max
layout: count 2, median 80 us, p95 120 us, max 120 us (std::ui layout)
paint: count 2, median 30 us, p95 300 us, max 300 us (std::ui paint, into the scene)
frame build: count 3, median 130 us, p95 450 us, max 450 us (the frame callback: std::ui's layout and paint, or the program's drawing)
encode: count 3, median 100 us, p95 200 us, max 200 us (scene to instances, glyphs and path masks rasterised)
submit: count 3, median 200 us, p95 300 us, max 300 us (upload, record and submit)
acquire: count 3, median 12 us, p95 16000 us, max 16000 us (waiting for the surface's next texture)
present: count 3, median 30 us, p95 40 us, max 40 us (handing the frame to the system)
interval: count 1, median 16667 us, p95 16667 us, max 16667 us (between presents); counted only between frames asked for back to back (the next frame requested before, or within 1 ms after, the previous present): 1 of 2 intervals
glyph atlas: 1 mask pages, 0 colour pages, 4.0 MiB (peak 4.0 MiB), 57 glyphs resident, 0 evicted, 0 dropped
path mask textures: 0 at most
peak per frame: 60 primitives, 40 quad instances, 120 sprite instances, 3 draw calls
instance buffers: 96.0 KiB at most
--- end ---
"
        );
    }

    #[test]
    fn a_request_within_the_slack_after_a_present_is_back_to_back_and_one_past_it_is_not() {
        let start = Instant::now();
        let at = |micros: u64| start + Duration::from_micros(micros);
        let mut report = Report::new(start);
        report.presented(1, &frame(1, 2, 0, 0), at(1_000));
        report.windows.entry(1).or_default().served = Some(at(2_000));
        report.presented(1, &frame(1, 2, 0, 0), at(5_000));
        report.windows.entry(1).or_default().served = Some(at(6_001));
        report.presented(1, &frame(1, 2, 0, 0), at(9_000));
        // A frame with no request at all (the system drew it unasked).
        report.presented(1, &frame(1, 2, 0, 0), at(9_500));
        assert_eq!(report.intervals_seen, 3);
        assert_eq!(report.interval.kept, vec![4_000]);
    }

    #[test]
    fn a_report_with_no_frame_says_so() {
        let text = Report::new(Instant::now()).exit_text(Duration::ZERO);
        assert!(
            text.contains("\nlayout: count 0 (std::ui layout)\n"),
            "{text}"
        );
        assert!(
            text.contains("\nglyph atlas: no frame presented\n"),
            "{text}"
        );
        assert!(text.contains("0 of 0 intervals"), "{text}");
    }

    #[test]
    fn the_start_report_from_fixed_facts() {
        let facts = StartFacts {
            window: 1,
            crate_version: "0.0.0",
            profile: "debug",
            os: "windows",
            arch: "x86_64",
            wgpu_backend: Some("dx12".to_string()),
            wgpu_debug: None,
            adapter: wgpu::AdapterInfo {
                name: "NVIDIA GeForce RTX 4070".to_string(),
                vendor: 0x10de,
                device: 0x2786,
                driver_info: "566.36".to_string(),
                ..wgpu::AdapterInfo::new(wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Dx12)
            },
            adapter_max_texture: 16384,
            device_max_texture: 16384,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            formats: vec![
                wgpu::TextureFormat::Bgra8UnormSrgb,
                wgpu::TextureFormat::Bgra8Unorm,
            ],
            present_mode: wgpu::PresentMode::AutoVsync,
            present_modes: vec![wgpu::PresentMode::Fifo, wgpu::PresentMode::Mailbox],
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            alpha_modes: vec![wgpu::CompositeAlphaMode::Opaque],
            frame_latency: 2,
            surface_size: (720, 600),
            inner_size: (720, 600),
            scale_factor: 1.5,
            text_scale: 1.0,
            monitor: Some(MonitorFacts {
                name: Some("\\\\.\\DISPLAY1".to_string()),
                refresh_millihertz: Some(143_998),
                size: (2560, 1440),
                scale_factor: 1.5,
            }),
        };
        assert_eq!(
            facts.text(),
            "\
--- vilan gfx report: start ---
window: 1
crate: vilan-rt-gfx 0.0.0 (debug build)
os: windows x86_64
WGPU_BACKEND: dx12
WGPU_DEBUG: (unset)
adapter: NVIDIA GeForce RTX 4070
adapter vendor: 0x10de (NVIDIA)
adapter device: 0x2786
adapter type: discrete GPU
backend: Dx12
driver: (none given)
driver info: 566.36
max texture side: 16384 (adapter 16384; the device asks WebGL2's limits, raised to the adapter's texture size)
surface format: Bgra8UnormSrgb (supported: Bgra8UnormSrgb, Bgra8Unorm)
present mode: AutoVsync (supported: Fifo, Mailbox)
alpha mode: Opaque (supported: Opaque)
frame latency: 2 frames at most
surface size: 720x600 px
window inner size: 720x600 px
scale factor: 1.5
text scale: 1 (the system's text size setting is not read)
monitor: \\\\.\\DISPLAY1, 2560x1440 px, 143.998 Hz, scale factor 1.5
--- end ---
"
        );
    }
}
