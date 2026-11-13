//! The Lucide sweep (gfx-path.md S4, its exit test): EVERY icon of the
//! generated package parses with [`Path::from_svg`], and its mask matches a
//! reference rasteriser's, `tiny-skia`'s, within a set error.
//!
//! # What is compared
//!
//! The package's data file (`vilan/packages/lucide/icons.tsv`, written by
//! `scripts/lucide-native.mjs` beside the Vilan source it renders to) holds
//! every icon's name, box, paint and path data. For each icon and size, our
//! mask is [`rasterize`]'s, the one the renderer draws; the reference is
//! `tiny-skia` drawing the same data, read by a parser of this module's own
//! (SVG's arc turned into cubics here, not by `zeno`), so a defect in the
//! crate's parser, its arcs, `zeno`'s stroker or its coverage shows as a
//! difference. Both are CPU masks, so the numbers are the same on every
//! machine: no GPU is involved.
//!
//! The error is the paper's (section 3.1): the mean absolute difference of
//! coverage (0 to 1) over the pixels either mask inks, and the worst pixel.
//! EVERY icon at 16, 24 and 48 dp (about 6,000 masks each way, a few
//! seconds in a debug build). The bounds were set from the whole set's
//! distribution at 1.64.0, with a margin, and are each well under what the
//! two stroker defects this sweep found did (`stroke_outline` in `mod.rs`):
//!
//! | dp | average of the means | the largest mean | the worst pixel |
//! |---|---|---|---|
//! | 16 | 0.051 | 0.099 | 0.62 |
//! | 24 | 0.027 | 0.102 | 0.53 |
//! | 48 | 0.014 | 0.057 | 0.69 |
//!
//! A dot drawn as nothing (Lucide's `h.01`) put `signal-zero` at a mean of
//! 0.81, and a 180 degree round join drawn flat put a pixel 0.82 to 1.0 off
//! (`numbers`, `braces`, `popcorn`). What stays is the two rasterisers'
//! difference (at 16 dp, against `tiny-skia` drawn 16 times larger and
//! averaged down, `zeno` is the closer of the two on straight edges and
//! `tiny-skia` on curves), and pixels where outlines lie over one another
//! (Lucide draws coincident segments, and a stroke's outline overlaps itself
//! at a join): `zeno` sums the coverage of overlapping edges where a
//! supersampling rasteriser counts the union, so those edge pixels are up to
//! about 0.7 darker.
//!
//! Set `VILAN_LUCIDE_SWEEP_REPORT=<dir>` to print the distribution, the
//! worst ten by mean and by pixel per size (each also against `tiny-skia`
//! at 16 times the size, averaged down, the nearest thing to the true
//! coverage at hand), and to write the worst side by side (ours, the
//! reference, the difference: red where ours inks more, blue where the
//! reference does) as PNG files into `<dir>`.

use std::path::PathBuf;

use super::{Cap, FillRule, Join, Path, PathMaskKey, PathPaint, Rasterized, Stroke, rasterize};

/// The largest mean difference an icon may have at any size.
const MEAN_BOUND: f64 = 0.12;
/// The largest difference at any one pixel.
const WORST_BOUND: f64 = 0.75;
/// Sizes in dp, drawn at 100%: the icon's box is the side.
const SIZES: [f64; 3] = [16.0, 24.0, 48.0];
/// The largest average, over the set, of the icons' means, per size.
const AVERAGE_BOUNDS: [f64; 3] = [0.06, 0.035, 0.02];

fn package() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("vilan")
        .join("packages")
        .join("lucide")
}

/// One icon of the data file.
struct Icon {
    name: String,
    identifier: String,
    view_box: f64,
    paint: PathPaint,
    data: String,
}

fn read_icons() -> Vec<Icon> {
    let file = package().join("icons.tsv");
    let text = std::fs::read_to_string(&file)
        .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
    let mut icons = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 5, "icons.tsv line {}: {line:?}", index + 1);
        icons.push(Icon {
            name: fields[0].to_string(),
            identifier: fields[1].to_string(),
            view_box: fields[2].parse().expect("a box"),
            paint: read_paint(fields[3]),
            data: fields[4].to_string(),
        });
    }
    icons
}

fn read_paint(field: &str) -> PathPaint {
    let mut words = field.split(' ');
    let kind = words.next().expect("a paint");
    let pairs: Vec<(&str, &str)> = words
        .map(|word| word.split_once('=').expect("key=value"))
        .collect();
    let value = |key: &str| {
        pairs
            .iter()
            .find(|(name, _)| *name == key)
            .unwrap_or_else(|| panic!("{field:?} has no {key}"))
            .1
    };
    match kind {
        "stroke" => PathPaint::Stroke(
            Stroke::new(value("width").parse().expect("a width"))
                .with_join(match value("join") {
                    "round" => Join::Round,
                    "miter" => Join::Miter,
                    "bevel" => Join::Bevel,
                    other => panic!("join {other}"),
                })
                .with_cap(match value("cap") {
                    "round" => Cap::Round,
                    "butt" => Cap::Butt,
                    "square" => Cap::Square,
                    other => panic!("cap {other}"),
                }),
        ),
        "fill" => PathPaint::Fill(match value("rule") {
            "nonzero" => FillRule::NonZero,
            "evenodd" => FillRule::EvenOdd,
            other => panic!("rule {other}"),
        }),
        other => panic!("paint {other}"),
    }
}

/// The names the Vilan source declares, in order: `export fun NAME(): Icon`.
fn source_names() -> Vec<String> {
    let file = package().join("src").join("icons").join("lib.vl");
    let text = std::fs::read_to_string(&file)
        .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
    text.lines()
        .filter_map(|line| {
            line.strip_prefix("export fun ")?
                .strip_suffix("(): Icon {")
                .map(str::to_string)
        })
        .collect()
}

// ------------------------------------------------------------ the reference ---

/// SVG path data as `tiny-skia` commands, by this module's own reading of
/// SVG 2's grammar (independent of `svg.rs`), arcs as cubics.
fn reference_path(data: &str) -> tiny_skia::Path {
    let mut reader = Reader {
        bytes: data.as_bytes(),
        at: 0,
    };
    let mut builder = tiny_skia::PathBuilder::new();
    let (mut current, mut start) = ((0.0, 0.0), (0.0, 0.0));
    // The last control point, for S and T's reflection.
    let mut last_cubic: Option<(f64, f64)> = None;
    let mut last_quad: Option<(f64, f64)> = None;
    reader.space();
    let mut command = b' ';
    while !reader.done() {
        if reader.peek().is_ascii_alphabetic() {
            command = reader.next();
        } else if command == b'M' {
            command = b'L';
        } else if command == b'm' {
            command = b'l';
        }
        let relative = command.is_ascii_lowercase();
        let base = if relative { current } else { (0.0, 0.0) };
        let point = |reader: &mut Reader| {
            let x = reader.number();
            let y = reader.number();
            (base.0 + x, base.1 + y)
        };
        let upper = command.to_ascii_uppercase();
        let (mut cubic, mut quad) = (None, None);
        match upper {
            b'M' => {
                current = point(&mut reader);
                start = current;
                builder.move_to(current.0 as f32, current.1 as f32);
            }
            b'L' => {
                current = point(&mut reader);
                line(&mut builder, current);
            }
            b'H' => {
                current.0 = base.0 + reader.number();
                line(&mut builder, current);
            }
            b'V' => {
                current.1 = base.1 + reader.number();
                line(&mut builder, current);
            }
            b'C' | b'S' => {
                let first = if upper == b'C' {
                    point(&mut reader)
                } else {
                    reflect(last_cubic, current)
                };
                let second = point(&mut reader);
                let end = point(&mut reader);
                cubic_to(&mut builder, first, second, end);
                cubic = Some(second);
                current = end;
            }
            b'Q' | b'T' => {
                let control = if upper == b'Q' {
                    point(&mut reader)
                } else {
                    reflect(last_quad, current)
                };
                let end = point(&mut reader);
                builder.quad_to(
                    control.0 as f32,
                    control.1 as f32,
                    end.0 as f32,
                    end.1 as f32,
                );
                quad = Some(control);
                current = end;
            }
            b'A' => {
                let radius_x = reader.number();
                let radius_y = reader.number();
                let rotation = reader.number();
                let large = reader.flag();
                let sweep = reader.flag();
                let end = point(&mut reader);
                arc(
                    &mut builder,
                    current,
                    (radius_x, radius_y),
                    rotation,
                    large,
                    sweep,
                    end,
                );
                current = end;
            }
            b'Z' => {
                builder.close();
                current = start;
            }
            other => panic!("{data:?}: command {}", other as char),
        }
        last_cubic = cubic;
        last_quad = quad;
        reader.space();
    }
    builder
        .finish()
        .unwrap_or_else(|| panic!("{data:?}: tiny-skia made no path"))
}

/// A line, or nothing for one of no length: `tiny-skia` would otherwise
/// draw a zero-length segment's round caps as SVG does, which is right too,
/// so both are kept by drawing the segment as given.
fn line(builder: &mut tiny_skia::PathBuilder, to: (f64, f64)) {
    builder.line_to(to.0 as f32, to.1 as f32);
}

fn cubic_to(
    builder: &mut tiny_skia::PathBuilder,
    first: (f64, f64),
    second: (f64, f64),
    end: (f64, f64),
) {
    builder.cubic_to(
        first.0 as f32,
        first.1 as f32,
        second.0 as f32,
        second.1 as f32,
        end.0 as f32,
        end.1 as f32,
    );
}

fn reflect(control: Option<(f64, f64)>, current: (f64, f64)) -> (f64, f64) {
    match control {
        Some((x, y)) => (2.0 * current.0 - x, 2.0 * current.1 - y),
        None => current,
    }
}

/// SVG's elliptical arc (appendix B.2.4's endpoint-to-centre conversion,
/// B.2.5's out-of-range rules), as cubics of at most a quarter turn each.
fn arc(
    builder: &mut tiny_skia::PathBuilder,
    from: (f64, f64),
    radii: (f64, f64),
    rotation_degrees: f64,
    large: bool,
    sweep: bool,
    to: (f64, f64),
) {
    if from == to {
        return;
    }
    let (mut radius_x, mut radius_y) = (radii.0.abs(), radii.1.abs());
    if radius_x == 0.0 || radius_y == 0.0 {
        line(builder, to);
        return;
    }
    let rotation = rotation_degrees.to_radians();
    let (sin, cos) = rotation.sin_cos();
    let half_x = (from.0 - to.0) / 2.0;
    let half_y = (from.1 - to.1) / 2.0;
    let x1 = cos * half_x + sin * half_y;
    let y1 = -sin * half_x + cos * half_y;
    let lambda = (x1 * x1) / (radius_x * radius_x) + (y1 * y1) / (radius_y * radius_y);
    if lambda > 1.0 {
        radius_x *= lambda.sqrt();
        radius_y *= lambda.sqrt();
    }
    let (rx2, ry2) = (radius_x * radius_x, radius_y * radius_y);
    let numerator = rx2 * ry2 - rx2 * y1 * y1 - ry2 * x1 * x1;
    let denominator = rx2 * y1 * y1 + ry2 * x1 * x1;
    let mut factor = (numerator / denominator).max(0.0).sqrt();
    if large == sweep {
        factor = -factor;
    }
    let centre_x1 = factor * radius_x * y1 / radius_y;
    let centre_y1 = -factor * radius_y * x1 / radius_x;
    let centre = (
        cos * centre_x1 - sin * centre_y1 + (from.0 + to.0) / 2.0,
        sin * centre_x1 + cos * centre_y1 + (from.1 + to.1) / 2.0,
    );
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let sign = if ux * vy - uy * vx < 0.0 { -1.0 } else { 1.0 };
        let dot = (ux * vx + uy * vy) / ((ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt());
        sign * dot.clamp(-1.0, 1.0).acos()
    };
    let start_x = (x1 - centre_x1) / radius_x;
    let start_y = (y1 - centre_y1) / radius_y;
    let end_x = (-x1 - centre_x1) / radius_x;
    let end_y = (-y1 - centre_y1) / radius_y;
    let theta = angle(1.0, 0.0, start_x, start_y);
    let mut delta = angle(start_x, start_y, end_x, end_y);
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }
    let pieces = (delta.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = delta / pieces as f64;
    let handle = 4.0 / 3.0 * (step / 4.0).tan();
    let on_ellipse = |angle: f64| {
        let (sin_angle, cos_angle) = angle.sin_cos();
        let x = radius_x * cos_angle;
        let y = radius_y * sin_angle;
        (centre.0 + cos * x - sin * y, centre.1 + sin * x + cos * y)
    };
    let derivative = |angle: f64| {
        let (sin_angle, cos_angle) = angle.sin_cos();
        let x = -radius_x * sin_angle;
        let y = radius_y * cos_angle;
        (cos * x - sin * y, sin * x + cos * y)
    };
    for piece in 0..pieces {
        let a = theta + step * piece as f64;
        let b = a + step;
        let (p0, p3) = (on_ellipse(a), on_ellipse(b));
        let (d0, d3) = (derivative(a), derivative(b));
        let first = (p0.0 + handle * d0.0, p0.1 + handle * d0.1);
        let second = (p3.0 - handle * d3.0, p3.1 - handle * d3.1);
        // The last piece ends exactly where the data says.
        let end = if piece + 1 == pieces { to } else { p3 };
        cubic_to(builder, first, second, end);
    }
}

/// The closed contours of `path` whose bounding box is at most `width` on
/// its narrower side.
fn narrow_closed_contours(path: &tiny_skia::Path, width: f32) -> Vec<tiny_skia::Path> {
    let mut found = Vec::new();
    let mut builder = tiny_skia::PathBuilder::new();
    let mut finish = |builder: &mut tiny_skia::PathBuilder, closed: bool| {
        let contour = std::mem::take(builder).finish();
        if let Some(contour) = contour
            && closed
        {
            let bounds = contour.bounds();
            if bounds.width().min(bounds.height()) <= width {
                found.push(contour);
            }
        }
    };
    let mut closed = false;
    for segment in path.segments() {
        match segment {
            tiny_skia::PathSegment::MoveTo(point) => {
                finish(&mut builder, closed);
                closed = false;
                builder.move_to(point.x, point.y);
            }
            tiny_skia::PathSegment::LineTo(point) => builder.line_to(point.x, point.y),
            tiny_skia::PathSegment::QuadTo(control, point) => {
                builder.quad_to(control.x, control.y, point.x, point.y);
            }
            tiny_skia::PathSegment::CubicTo(first, second, point) => {
                builder.cubic_to(first.x, first.y, second.x, second.y, point.x, point.y);
            }
            tiny_skia::PathSegment::Close => {
                builder.close();
                closed = true;
            }
        }
    }
    finish(&mut builder, closed);
    found
}

struct Reader<'data> {
    bytes: &'data [u8],
    at: usize,
}

impl Reader<'_> {
    fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }

    fn peek(&self) -> u8 {
        self.bytes[self.at]
    }

    fn next(&mut self) -> u8 {
        let byte = self.bytes[self.at];
        self.at += 1;
        self.space();
        byte
    }

    fn space(&mut self) {
        while !self.done() && (self.peek().is_ascii_whitespace() || self.peek() == b',') {
            self.at += 1;
        }
    }

    fn number(&mut self) -> f64 {
        let start = self.at;
        let mut end = start;
        let bytes = self.bytes;
        if end < bytes.len() && (bytes[end] == b'-' || bytes[end] == b'+') {
            end += 1;
        }
        let mut dot = false;
        while end < bytes.len() && (bytes[end].is_ascii_digit() || (bytes[end] == b'.' && !dot)) {
            dot |= bytes[end] == b'.';
            end += 1;
        }
        if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
            end += 1;
            if end < bytes.len() && (bytes[end] == b'-' || bytes[end] == b'+') {
                end += 1;
            }
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
        }
        let text = std::str::from_utf8(&bytes[start..end]).expect("ASCII");
        self.at = end;
        self.space();
        text.parse()
            .unwrap_or_else(|_| panic!("not a number at byte {start}: {text:?}"))
    }

    fn flag(&mut self) -> bool {
        let byte = self.bytes[self.at];
        self.at += 1;
        self.space();
        match byte {
            b'0' => false,
            b'1' => true,
            other => panic!("not a flag: {}", other as char),
        }
    }
}

// ---------------------------------------------------------------- the masks ---

/// A coverage canvas: `side` pixels square, 0 to 255.
struct Canvas {
    side: u32,
    pixels: Vec<u8>,
}

/// The canvas both masks are placed on, for an icon at `scale` device
/// pixels per path unit: its box plus the stroke's reach and a margin on
/// every side, the path's origin at (`margin`, `margin`).
fn margin(paint: &PathPaint, scale: f64) -> u32 {
    (f64::from(paint.reach()) * scale).ceil() as u32 + 2
}

fn ours(path: &Path, paint: PathPaint, scale: f64, view_box: f64) -> Canvas {
    let margin = margin(&paint, scale);
    let side = (view_box * scale).ceil() as u32 + 2 * margin;
    let mut canvas = Canvas {
        side,
        pixels: vec![0; (side * side) as usize],
    };
    let key = PathMaskKey::new(path, paint, scale as f32);
    match rasterize(&key, &mut zeno::Scratch::new(), 4096) {
        Rasterized::Mask(mask) => {
            for row in 0..mask.height {
                for column in 0..mask.width {
                    let x = i64::from(margin) + i64::from(mask.left) + i64::from(column);
                    let y = i64::from(margin) + i64::from(mask.top) + i64::from(row);
                    assert!(
                        (0..i64::from(side)).contains(&x) && (0..i64::from(side)).contains(&y),
                        "a mask pixel outside the box and its stroke's reach"
                    );
                    canvas.pixels[(y as u32 * side + x as u32) as usize] =
                        mask.pixels[(row * mask.width + column) as usize];
                }
            }
        }
        Rasterized::Empty => {}
        Rasterized::TooLarge => panic!("an icon's mask too large"),
    }
    canvas
}

/// `tiny-skia`'s mask of the icon on the same canvas, drawn `supersample`
/// times larger on each axis and averaged down (1: drawn at the size).
fn reference(data: &str, paint: PathPaint, scale: f64, view_box: f64, supersample: u32) -> Canvas {
    let margin = margin(&paint, scale);
    let side = (view_box * scale).ceil() as u32 + 2 * margin;
    let large = side * supersample;
    let mut pixmap = tiny_skia::Pixmap::new(large, large).expect("a pixmap");
    let path = reference_path(data);
    let mut ink = tiny_skia::Paint::default();
    ink.set_color_rgba8(0, 0, 0, 255);
    ink.anti_alias = true;
    let factor = f64::from(supersample);
    let transform = tiny_skia::Transform::from_row(
        (scale * factor) as f32,
        0.0,
        0.0,
        (scale * factor) as f32,
        (f64::from(margin) * factor) as f32,
        (f64::from(margin) * factor) as f32,
    );
    match paint {
        PathPaint::Fill(rule) => pixmap.fill_path(
            &path,
            &ink,
            match rule {
                FillRule::NonZero => tiny_skia::FillRule::Winding,
                FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
            },
            transform,
            None,
        ),
        PathPaint::Stroke(stroke) => pixmap.stroke_path(
            &path,
            &ink,
            &tiny_skia::Stroke {
                width: stroke.width,
                miter_limit: stroke.miter_limit,
                line_cap: match stroke.cap {
                    Cap::Butt => tiny_skia::LineCap::Butt,
                    Cap::Round => tiny_skia::LineCap::Round,
                    Cap::Square => tiny_skia::LineCap::Square,
                },
                line_join: match stroke.join {
                    Join::Miter => tiny_skia::LineJoin::Miter,
                    Join::Round => tiny_skia::LineJoin::Round,
                    Join::Bevel => tiny_skia::LineJoin::Bevel,
                },
                dash: None,
            },
            transform,
            None,
        ),
    }
    // What SVG strokes and `tiny-skia`'s stroker leaves out: a closed
    // contour narrower than the stroke is inked inside too (from any point
    // inside it the contour is within half its narrower side, along that
    // axis, so within half the width). `tiny-skia` strokes Lucide's dots (a
    // circle of radius 0.5, 2 wide) as a ring with a hole; upstream also
    // FILLS those dots, a fill the generator drops for exactly this reason.
    if let PathPaint::Stroke(stroke) = paint {
        for contour in narrow_closed_contours(&path, stroke.width) {
            pixmap.fill_path(
                &contour,
                &ink,
                tiny_skia::FillRule::Winding,
                transform,
                None,
            );
        }
    }
    let alpha: Vec<u8> = pixmap.pixels().iter().map(|pixel| pixel.alpha()).collect();
    if supersample == 1 {
        return Canvas {
            side,
            pixels: alpha,
        };
    }
    let mut pixels = vec![0u8; (side * side) as usize];
    let samples = f64::from(supersample * supersample);
    for y in 0..side {
        for x in 0..side {
            let mut sum = 0u32;
            for dy in 0..supersample {
                for dx in 0..supersample {
                    let index = (y * supersample + dy) * large + x * supersample + dx;
                    sum += u32::from(alpha[index as usize]);
                }
            }
            pixels[(y * side + x) as usize] = (f64::from(sum) / samples).round() as u8;
        }
    }
    Canvas { side, pixels }
}

/// The paper's error: the mean absolute coverage difference over the pixels
/// either canvas inks, and the worst pixel, both 0 to 1.
fn error(ours: &Canvas, reference: &Canvas) -> (f64, f64) {
    let (mut sum, mut count, mut worst) = (0.0, 0usize, 0.0f64);
    for (a, b) in ours.pixels.iter().zip(&reference.pixels) {
        if *a == 0 && *b == 0 {
            continue;
        }
        let difference = (f64::from(*a) - f64::from(*b)).abs() / 255.0;
        sum += difference;
        count += 1;
        worst = worst.max(difference);
    }
    if count == 0 {
        (0.0, 0.0)
    } else {
        (sum / count as f64, worst)
    }
}

/// One icon at one size.
struct Measured {
    name: String,
    dp: f64,
    mean: f64,
    worst: f64,
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index]
}

#[test]
fn every_lucide_icon_parses_and_draws_within_a_set_error_of_a_reference_rasteriser() {
    let icons = read_icons();
    // The data and the source agree: the same icons, in the same order.
    let names: Vec<String> = icons.iter().map(|icon| icon.identifier.clone()).collect();
    let declared = source_names();
    assert_eq!(
        names.len(),
        declared.len(),
        "icons.tsv has {} icons and src/icons/lib.vl declares {}: regenerate both \
         (vilan/packages/lucide/README.md)",
        names.len(),
        declared.len()
    );
    assert_eq!(
        names, declared,
        "icons.tsv and src/icons/lib.vl name different icons"
    );
    assert!(
        icons.len() > 1000,
        "only {} icons: the sweep would be thin",
        icons.len()
    );

    let mut measured = Vec::new();
    let mut failures = Vec::new();
    for icon in &icons {
        let path = match Path::from_svg(&icon.data) {
            Ok(path) => path,
            Err(error) => {
                failures.push(format!("{}: does not parse: {error}", icon.name));
                continue;
            }
        };
        for dp in SIZES {
            let scale = dp / icon.view_box;
            let ours = ours(&path, icon.paint, scale, icon.view_box);
            let reference = reference(&icon.data, icon.paint, scale, icon.view_box, 1);
            let (mean, worst) = error(&ours, &reference);
            if mean > MEAN_BOUND || worst > WORST_BOUND {
                failures.push(format!(
                    "{} at {dp} dp: mean {mean:.4} (bound {MEAN_BOUND}), worst {worst:.3} \
                     (bound {WORST_BOUND})",
                    icon.name
                ));
            }
            measured.push(Measured {
                name: icon.name.clone(),
                dp,
                mean,
                worst,
            });
        }
    }

    for (dp, bound) in SIZES.iter().zip(AVERAGE_BOUNDS) {
        let at: Vec<f64> = measured
            .iter()
            .filter(|entry| entry.dp == *dp)
            .map(|entry| entry.mean)
            .collect();
        let average = at.iter().sum::<f64>() / at.len() as f64;
        if average > bound {
            failures.push(format!(
                "at {dp} dp the icons' means average {average:.4} (bound {bound})"
            ));
        }
    }

    if let Some(directory) = std::env::var_os("VILAN_LUCIDE_SWEEP_REPORT") {
        report(&icons, &measured, &PathBuf::from(directory));
    }
    assert!(
        failures.is_empty(),
        "{} of {} icon sizes off the reference:\n{}",
        failures.len(),
        measured.len(),
        failures.join("\n")
    );
}

/// The distribution per size, the worst ten, and the worst few as images.
fn report(icons: &[Icon], measured: &[Measured], directory: &std::path::Path) {
    std::fs::create_dir_all(directory).expect("the report directory");
    for dp in SIZES {
        let at: Vec<&Measured> = measured.iter().filter(|entry| entry.dp == dp).collect();
        let mut means: Vec<f64> = at.iter().map(|entry| entry.mean).collect();
        let mut worsts: Vec<f64> = at.iter().map(|entry| entry.worst).collect();
        means.sort_by(f64::total_cmp);
        worsts.sort_by(f64::total_cmp);
        let average = means.iter().sum::<f64>() / means.len() as f64;
        println!(
            "{dp} dp: {} icons; mean error: average {average:.4}, median {:.4}, p99 {:.4}, max \
             {:.4}; worst pixel: median {:.3}, p99 {:.3}, max {:.3}",
            at.len(),
            percentile(&means, 0.5),
            percentile(&means, 0.99),
            means[means.len() - 1],
            percentile(&worsts, 0.5),
            percentile(&worsts, 0.99),
            worsts[worsts.len() - 1],
        );
        let mut by_mean = at.clone();
        by_mean.sort_by(|a, b| b.mean.total_cmp(&a.mean).then(a.name.cmp(&b.name)));
        for entry in by_mean.iter().take(10) {
            println!(
                "  worst mean at {dp} dp: {} mean {:.4} worst {:.3}",
                entry.name, entry.mean, entry.worst
            );
        }
        let mut by_worst = at.clone();
        by_worst.sort_by(|a, b| b.worst.total_cmp(&a.worst).then(a.name.cmp(&b.name)));
        for entry in by_worst.iter().take(10) {
            println!(
                "  worst pixel at {dp} dp: {} mean {:.4} worst {:.3}",
                entry.name, entry.mean, entry.worst
            );
        }
        for entry in by_mean.iter().take(10).chain(by_worst.iter().take(10)) {
            let icon = icons
                .iter()
                .find(|icon| icon.name == entry.name)
                .expect("a measured icon");
            let scale = dp / icon.view_box;
            let path = Path::from_svg(&icon.data).expect("parsed above");
            let truth = reference(&icon.data, icon.paint, scale, icon.view_box, 16);
            let ours = ours(&path, icon.paint, scale, icon.view_box);
            let direct = reference(&icon.data, icon.paint, scale, icon.view_box, 1);
            let (ours_mean, ours_worst) = error(&ours, &truth);
            let (direct_mean, direct_worst) = error(&direct, &truth);
            println!(
                "  against tiny-skia at 16x, {} at {dp} dp: ours mean {ours_mean:.4} worst \
                 {ours_worst:.3}; tiny-skia mean {direct_mean:.4} worst {direct_worst:.3}",
                entry.name
            );
        }
        for entry in by_mean.iter().take(4).chain(by_worst.iter().take(4)) {
            let icon = icons
                .iter()
                .find(|icon| icon.name == entry.name)
                .expect("a measured icon");
            write_side_by_side(icon, dp, directory);
        }
    }
}

/// Ours, the reference and the difference (red where ours inks more, blue
/// where the reference does), each magnified eight times, dark on white.
fn write_side_by_side(icon: &Icon, dp: f64, directory: &std::path::Path) {
    const MAGNIFY: u32 = 8;
    let scale = dp / icon.view_box;
    let path = Path::from_svg(&icon.data).expect("parsed above");
    let ours = ours(&path, icon.paint, scale, icon.view_box);
    let reference = reference(&icon.data, icon.paint, scale, icon.view_box, 1);
    let side = ours.side;
    let width = (side * 3 + 2) * MAGNIFY;
    let height = side * MAGNIFY;
    let mut pixels = vec![255u8; (width * height * 4) as usize];
    for y in 0..side {
        for x in 0..side {
            let a = ours.pixels[(y * side + x) as usize];
            let b = reference.pixels[(y * side + x) as usize];
            let difference = i32::from(a) - i32::from(b);
            let panels: [[u8; 3]; 3] = [
                [255 - a, 255 - a, 255 - a],
                [255 - b, 255 - b, 255 - b],
                if difference > 0 {
                    let shade = (255 - (difference * 4).min(255)) as u8;
                    [255, shade, shade]
                } else {
                    let shade = (255 + (difference * 4).max(-255)) as u8;
                    [shade, shade, 255]
                },
            ];
            for (panel, colour) in panels.iter().enumerate() {
                let left = (panel as u32 * (side + 1)) * MAGNIFY;
                for dy in 0..MAGNIFY {
                    for dx in 0..MAGNIFY {
                        let at =
                            (((y * MAGNIFY + dy) * width + left + x * MAGNIFY + dx) * 4) as usize;
                        pixels[at..at + 3].copy_from_slice(colour);
                    }
                }
            }
        }
    }
    let file = directory.join(format!("{}-{dp}.png", icon.name));
    let output = std::io::BufWriter::new(std::fs::File::create(&file).expect("the image"));
    let mut encoder = png::Encoder::new(output, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&pixels))
        .expect("write the image");
}
