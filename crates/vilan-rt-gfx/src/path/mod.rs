//! Paths: vector shapes made of lines, curves and arcs, filled or stroked in
//! one solid colour (gfx-path.md; its section 9 RULED 2026-11-12, R1 to R8
//! here).
//!
//! # How a path is drawn
//!
//! As a COVERAGE MASK (R1): the path is rasterised on the CPU by `zeno` (R2),
//! the rasteriser `swash` draws glyphs with, at the device size it is drawn
//! at, into the glyph atlas's mask pages under a key of its own
//! ([`PathMaskKey`]), and drawn as one tinted textured quad by the sprite
//! pipeline glyphs use. An icon therefore costs what a glyph costs once its
//! mask is made, batches with the text beside it, and blends the same way (in
//! linear light). A mask over [`LARGEST_ATLAS_MASK`] device pixels a side gets
//! a texture of its own instead (R8), freed after a frame that does not draw
//! it (`masks.rs`).
//!
//! # Units and placement
//!
//! A path's coordinates are its own units: an icon's box (24 for Lucide), or
//! dp for a chart drawn at scale 1. [`SceneBuilder::path`] draws it with its
//! origin at a point in dp and `scale` dp to one path unit, so a stroke's
//! width (in path units) scales with the path, as SVG's does. The renderer
//! multiplies that by the transform's scale and the display's content scale
//! into one device scale, rasterises at exactly that scale (a path is sharp
//! at every size), and SNAPS THE ORIGIN to a whole device pixel on both axes,
//! as a text baseline is snapped (R7). There are no subpixel bins for paths:
//! a path moved by an animation moves by whole pixels.
//!
//! # Identity and caching
//!
//! A [`Path`] keeps its commands and a content hash made once. A mask's key
//! is that hash, the paint and the device scale, and the commands are
//! compared on a hit, so two `Path` values parsed from the same data share
//! one mask. A path that is STROKED keeps its stroke's outline per stroke
//! style, made the first time it is drawn with that style: the outline does
//! not depend on the size it is drawn at, so every later size only fills it
//! (the masks are byte-identical to stroking directly).
//!
//! # What a path is not
//!
//! Solid colour only, and nothing is clipped TO a path (R6): a path is
//! clipped by the clip in force, like every primitive. No dashes (R5). No
//! rotation (the crate's transform is a translation and a uniform scale).

pub(crate) mod masks;
mod svg;

use std::sync::{Arc, Mutex, PoisonError};

use zeno::{Command as Segment, PathBuilder as _};

use crate::geometry::{Point, Rect};

pub(crate) use masks::{MaskStorage, PathMasks, PlacedMask};

/// How a fill decides what is inside a path whose contours cross or nest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FillRule {
    /// Inside where the contours wind around a point a non-zero number of
    /// times: a contour inside another in the same direction is filled.
    NonZero,
    /// Inside where a ray from a point crosses an odd number of contours: a
    /// contour inside another is a hole, whatever its direction.
    EvenOdd,
}

/// How a stroke turns a corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Join {
    /// The outer edges extended until they meet, or a bevel where that point
    /// is further than the miter limit allows. (`zeno` bevels every corner
    /// sharper than 90 degrees whatever the limit: see the crate's known
    /// limits.)
    Miter,
    Round,
    Bevel,
}

/// How a stroke ends an open contour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Cap {
    Butt,
    Round,
    Square,
}

/// A stroke, in the PATH's units: it scales with the path, as SVG's does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub join: Join,
    pub cap: Cap,
    /// The longest a miter may be, as a multiple of the width (SVG's
    /// `stroke-miterlimit`); at least 1. Read only for [`Join::Miter`].
    pub miter_limit: f32,
}

impl Stroke {
    /// `width` wide with round joins and round caps (an icon set's usual
    /// stroke) and SVG's miter limit of 4.
    pub fn new(width: f32) -> Stroke {
        Stroke {
            width,
            join: Join::Round,
            cap: Cap::Round,
            miter_limit: 4.0,
        }
    }

    pub fn with_join(self, join: Join) -> Stroke {
        Stroke { join, ..self }
    }

    pub fn with_cap(self, cap: Cap) -> Stroke {
        Stroke { cap, ..self }
    }

    pub fn with_miter_limit(self, miter_limit: f32) -> Stroke {
        Stroke {
            miter_limit,
            ..self
        }
    }

    /// Whether the scene may hold it: a finite width that is not negative
    /// (zero draws nothing) and a finite miter limit of at least 1.
    pub(crate) fn is_valid(&self) -> bool {
        self.width.is_finite()
            && self.width >= 0.0
            && self.miter_limit.is_finite()
            && self.miter_limit >= 1.0
    }

    /// How far the stroke's ink can reach beyond the path's points, in path
    /// units: half the width, times the miter limit for miter joins and times
    /// the square root of two for square caps.
    pub(crate) fn reach(&self) -> f32 {
        let mut factor: f32 = 1.0;
        if self.join == Join::Miter {
            factor = factor.max(self.miter_limit);
        }
        if self.cap == Cap::Square {
            factor = factor.max(std::f32::consts::SQRT_2);
        }
        self.width.max(0.0) / 2.0 * factor
    }

    fn key(&self) -> StrokeKey {
        StrokeKey {
            width_bits: self.width.to_bits(),
            join: self.join,
            cap: self.cap,
            // The limit means nothing to the other joins, so strokes that
            // differ only there share an outline and a mask.
            miter_limit_bits: if self.join == Join::Miter {
                self.miter_limit.to_bits()
            } else {
                0
            },
        }
    }

    fn zeno(&self) -> zeno::Stroke<'static> {
        let mut stroke = zeno::Stroke::new(self.width);
        stroke
            .join(match self.join {
                Join::Miter => zeno::Join::Miter,
                Join::Round => zeno::Join::Round,
                Join::Bevel => zeno::Join::Bevel,
            })
            .cap(match self.cap {
                Cap::Butt => zeno::Cap::Butt,
                Cap::Round => zeno::Cap::Round,
                Cap::Square => zeno::Cap::Square,
            })
            .miter_limit(self.miter_limit);
        stroke
    }
}

/// How a path is painted: its inside filled by a rule, or its outline
/// stroked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathPaint {
    Fill(FillRule),
    Stroke(Stroke),
}

impl PathPaint {
    /// How far the ink can reach beyond the path's points, in path units.
    pub(crate) fn reach(&self) -> f32 {
        match self {
            PathPaint::Fill(_) => 0.0,
            PathPaint::Stroke(stroke) => stroke.reach(),
        }
    }

    pub(crate) fn key(&self) -> PaintKey {
        match self {
            PathPaint::Fill(rule) => PaintKey::Fill(*rule),
            PathPaint::Stroke(stroke) => PaintKey::Stroke(stroke.key()),
        }
    }
}

/// A stroke style compared exactly (its numbers as bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct StrokeKey {
    width_bits: u32,
    join: Join,
    cap: Cap,
    miter_limit_bits: u32,
}

/// A paint compared exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum PaintKey {
    Fill(FillRule),
    Stroke(StrokeKey),
}

/// Why a path could not be made.
#[derive(Clone, Debug, PartialEq)]
pub enum PathError {
    /// SVG path data that stops making sense at byte `byte` (the data's
    /// length when it ends too soon).
    Syntax {
        byte: usize,
        reason: &'static str,
        /// The data around the byte, for the message.
        excerpt: String,
    },
    /// A builder's segment number `segment` (from 0) had a coordinate, a
    /// radius or a rotation that is not a finite number.
    NotFinite { segment: usize },
}

impl PathError {
    /// The byte a syntax error names.
    pub fn byte(&self) -> Option<usize> {
        match self {
            PathError::Syntax { byte, .. } => Some(*byte),
            PathError::NotFinite { .. } => None,
        }
    }
}

impl std::fmt::Display for PathError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathError::Syntax {
                byte,
                reason,
                excerpt,
            } => write!(
                formatter,
                "invalid path data at byte {byte}: {reason}: {excerpt}"
            ),
            PathError::NotFinite { segment } => write!(
                formatter,
                "a path's segment {segment} has a coordinate, radius or rotation that is not a \
                 finite number"
            ),
        }
    }
}

impl std::error::Error for PathError {}

/// Up to this many bytes of the data on each side of an error's byte.
const EXCERPT_REACH: usize = 16;

/// The data around `byte`, with `>` before the byte (or at the end) and
/// `...` where it was cut, on character boundaries.
fn excerpt(data: &str, byte: usize) -> String {
    let mut start = byte.saturating_sub(EXCERPT_REACH);
    while !data.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (byte + EXCERPT_REACH).min(data.len());
    while !data.is_char_boundary(end) {
        end += 1;
    }
    let mut text = String::new();
    if start > 0 {
        text.push_str("...");
    }
    text.push_str(&data[start..byte]);
    text.push('>');
    text.push_str(&data[byte..end]);
    if end < data.len() {
        text.push_str("...");
    }
    text
}

/// The commands of a path being made, with what SVG's rules need: the
/// current point, the start of the contour (a close returns to it), and
/// whether a contour is open. Every contour starts with an explicit move,
/// so `zeno` never has to guess where one begins.
pub(crate) struct Commands {
    segments: Vec<Segment>,
    current: Point,
    start: Point,
    open: bool,
}

fn zeno_point(point: Point) -> zeno::Point {
    zeno::Point::new(point.x, point.y)
}

impl Commands {
    pub(crate) fn new() -> Commands {
        Commands {
            segments: Vec::new(),
            current: Point::ZERO,
            start: Point::ZERO,
            open: false,
        }
    }

    pub(crate) fn current(&self) -> Point {
        self.current
    }

    /// A drawing segment with no contour open starts one at the current
    /// point (the start of the contour just closed, or the origin).
    fn ensure_contour(&mut self) {
        if !self.open {
            self.segments
                .push(Segment::MoveTo(zeno_point(self.current)));
            self.start = self.current;
            self.open = true;
        }
    }

    pub(crate) fn move_to(&mut self, to: Point) {
        self.segments.push(Segment::MoveTo(zeno_point(to)));
        self.current = to;
        self.start = to;
        self.open = true;
    }

    pub(crate) fn line_to(&mut self, to: Point) {
        self.ensure_contour();
        self.segments.push(Segment::LineTo(zeno_point(to)));
        self.current = to;
    }

    pub(crate) fn quad_to(&mut self, control: Point, to: Point) {
        self.ensure_contour();
        self.segments
            .push(Segment::QuadTo(zeno_point(control), zeno_point(to)));
        self.current = to;
    }

    pub(crate) fn cubic_to(&mut self, first: Point, second: Point, to: Point) {
        self.ensure_contour();
        self.segments.push(Segment::CurveTo(
            zeno_point(first),
            zeno_point(second),
            zeno_point(to),
        ));
        self.current = to;
    }

    /// SVG's elliptical arc (its appendix B.2.5, "out-of-range parameters"):
    /// an end point equal to the current point draws nothing, a zero radius
    /// is a straight line, negative radii count as positive, and radii too
    /// small to reach the end point are scaled up until they do (`zeno`'s
    /// arc does the last two). The arc's cubic curves end EXACTLY at `to`,
    /// so a relative command after it is relative to the point the data
    /// says.
    pub(crate) fn arc_to(
        &mut self,
        radius_x: f32,
        radius_y: f32,
        rotation_degrees: f32,
        large: bool,
        sweep: bool,
        to: Point,
    ) {
        if to == self.current {
            return;
        }
        if radius_x == 0.0 || radius_y == 0.0 {
            self.line_to(to);
            return;
        }
        self.ensure_contour();
        let before = self.segments.len();
        self.segments.arc_to(
            radius_x,
            radius_y,
            zeno::Angle::from_degrees(rotation_degrees),
            if large {
                zeno::ArcSize::Large
            } else {
                zeno::ArcSize::Small
            },
            if sweep {
                zeno::ArcSweep::Positive
            } else {
                zeno::ArcSweep::Negative
            },
            zeno_point(to),
        );
        let drawn = self.segments.len() > before;
        match self.segments.last_mut() {
            Some(Segment::CurveTo(_, _, end)) if drawn => {
                *end = zeno_point(to);
            }
            // `zeno` draws nothing only for an end point equal to the start,
            // which is refused above; a line keeps the contour honest anyway.
            _ => self.segments.push(Segment::LineTo(zeno_point(to))),
        }
        self.current = to;
    }

    /// Closes the open contour; the current point returns to its start. A
    /// close with no contour open does nothing.
    pub(crate) fn close(&mut self) {
        if self.open {
            self.segments.push(Segment::Close);
            self.current = self.start;
            self.open = false;
        }
    }

    fn finish(self) -> Path {
        Path::from_segments(self.segments)
    }
}

/// A shape made of lines, curves and arcs, in its own units: an immutable
/// value, cheap to clone (its commands are shared). Made from SVG path data
/// ([`Path::from_svg`]) or with a [`PathBuilder`]; drawn with
/// [`SceneBuilder::path`](crate::SceneBuilder::path).
#[derive(Clone, Debug)]
pub struct Path {
    pub(crate) data: Arc<PathData>,
}

#[derive(Debug)]
pub(crate) struct PathData {
    pub(crate) segments: Vec<Segment>,
    /// FNV-1a over the segments, made once: the mask key's first word.
    pub(crate) hash: u64,
    /// Every point, control points included; `None` for an empty path.
    bounds: Option<Rect>,
    /// Stroke outlines made so far, most recently used last.
    outlines: Mutex<Vec<(StrokeKey, Arc<[Segment]>)>>,
}

/// Outlines a path keeps: a path drawn with more stroke styles than this
/// (an animated width) makes the oldest again when it comes back.
const KEPT_OUTLINES: usize = 4;

/// FNV-1a, 64 bits: the same hash on every run and every machine (the
/// standard library's is seeded per process).
struct Fnv(u64);

impl Fnv {
    fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn points(&mut self, tag: u8, points: &[&zeno::Point]) {
        self.bytes(&[tag]);
        for point in points {
            self.bytes(&point.x.to_bits().to_le_bytes());
            self.bytes(&point.y.to_bits().to_le_bytes());
        }
    }
}

fn hash_segments(segments: &[Segment]) -> u64 {
    let mut hash = Fnv(0xcbf2_9ce4_8422_2325);
    for segment in segments {
        match segment {
            Segment::MoveTo(to) => hash.points(0, &[to]),
            Segment::LineTo(to) => hash.points(1, &[to]),
            Segment::QuadTo(control, to) => hash.points(2, &[control, to]),
            Segment::CurveTo(first, second, to) => hash.points(3, &[first, second, to]),
            Segment::Close => hash.points(4, &[]),
        }
    }
    hash.0
}

fn segment_bounds(segments: &[Segment]) -> Option<Rect> {
    let mut extent: Option<[f32; 4]> = None;
    let mut include = |point: &zeno::Point| {
        extent = Some(match extent {
            None => [point.x, point.y, point.x, point.y],
            Some([left, top, right, bottom]) => [
                left.min(point.x),
                top.min(point.y),
                right.max(point.x),
                bottom.max(point.y),
            ],
        });
    };
    for segment in segments {
        match segment {
            Segment::MoveTo(to) | Segment::LineTo(to) => include(to),
            Segment::QuadTo(control, to) => {
                include(control);
                include(to);
            }
            Segment::CurveTo(first, second, to) => {
                include(first);
                include(second);
                include(to);
            }
            Segment::Close => {}
        }
    }
    extent.map(|[left, top, right, bottom]| Rect::new(left, top, right - left, bottom - top))
}

impl Path {
    fn from_segments(segments: Vec<Segment>) -> Path {
        Path {
            data: Arc::new(PathData {
                hash: hash_segments(&segments),
                bounds: segment_bounds(&segments),
                segments,
                outlines: Mutex::new(Vec::new()),
            }),
        }
    }

    /// SVG path data (`M20 6 9 17l-5-5`): every command, absolute and
    /// relative, arcs included (see `svg.rs` for the grammar). The error
    /// names the byte where the data stops making sense.
    pub fn from_svg(data: &str) -> Result<Path, PathError> {
        let mut commands = Commands::new();
        match svg::parse(data, &mut commands) {
            Ok(()) => Ok(commands.finish()),
            Err(error) => Err(PathError::Syntax {
                byte: error.byte,
                reason: error.reason,
                excerpt: excerpt(data, error.byte),
            }),
        }
    }

    pub fn builder() -> PathBuilder {
        PathBuilder::new()
    }

    /// The smallest rectangle holding every point, control points included,
    /// in the path's own units; an empty rectangle at the origin for an
    /// empty path.
    pub fn bounds(&self) -> Rect {
        self.data.bounds.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0))
    }

    /// Whether it has no commands (it draws nothing).
    pub fn is_empty(&self) -> bool {
        self.data.segments.is_empty()
    }

    /// Whether `self` and `other` hold the same commands (as two paths parsed
    /// from the same data do), and so share their masks.
    pub fn same_shape(&self, other: &Path) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
            || (self.data.hash == other.data.hash && self.data.segments == other.data.segments)
    }

    /// The outline of this path stroked with `stroke`, made once per stroke
    /// style and kept (see the module documentation).
    pub(crate) fn outline(&self, stroke: &Stroke) -> Arc<[Segment]> {
        let key = stroke.key();
        {
            let mut kept = self
                .data
                .outlines
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if let Some(index) = kept.iter().position(|(style, _)| *style == key) {
                let entry = kept.remove(index);
                let outline = entry.1.clone();
                kept.push(entry);
                return outline;
            }
        }
        // Made outside the lock, inserted whole.
        let mut outline: Vec<Segment> = Vec::new();
        zeno::apply(&self.data.segments[..], stroke.zeno(), None, &mut outline);
        let outline: Arc<[Segment]> = Arc::from(outline);
        let mut kept = self
            .data
            .outlines
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if kept.len() >= KEPT_OUTLINES {
            kept.remove(0);
        }
        kept.push((key, outline.clone()));
        outline
    }

    /// The stroke styles whose outlines are kept now, oldest first.
    #[cfg(test)]
    pub(crate) fn kept_outlines(&self) -> usize {
        self.data
            .outlines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

/// Builds a [`Path`] one segment at a time, by SVG's rules: a segment with
/// no contour open starts one at the current point (the origin at first, the
/// start of the contour a [`close`](PathBuilder::close) just ended), and an
/// arc is SVG's elliptical arc. A coordinate that is not a finite number is
/// recorded and reported once, by [`finish`](PathBuilder::finish).
pub struct PathBuilder {
    commands: Commands,
    segments_given: usize,
    not_finite: Option<usize>,
}

impl Default for PathBuilder {
    fn default() -> Self {
        PathBuilder::new()
    }
}

impl PathBuilder {
    pub fn new() -> PathBuilder {
        PathBuilder {
            commands: Commands::new(),
            segments_given: 0,
            not_finite: None,
        }
    }

    /// Counts a segment; answers whether its numbers are all finite (and
    /// records the first that is not).
    fn accept(&mut self, numbers: &[f32]) -> bool {
        let segment = self.segments_given;
        self.segments_given += 1;
        let finite = numbers.iter().all(|number| number.is_finite());
        if !finite && self.not_finite.is_none() {
            self.not_finite = Some(segment);
        }
        finite && self.not_finite.is_none()
    }

    pub fn move_to(&mut self, x: f32, y: f32) -> &mut PathBuilder {
        if self.accept(&[x, y]) {
            self.commands.move_to(Point::new(x, y));
        }
        self
    }

    pub fn line_to(&mut self, x: f32, y: f32) -> &mut PathBuilder {
        if self.accept(&[x, y]) {
            self.commands.line_to(Point::new(x, y));
        }
        self
    }

    /// A quadratic curve through the control point `(control_x, control_y)`.
    pub fn quad_to(&mut self, control_x: f32, control_y: f32, x: f32, y: f32) -> &mut PathBuilder {
        if self.accept(&[control_x, control_y, x, y]) {
            self.commands
                .quad_to(Point::new(control_x, control_y), Point::new(x, y));
        }
        self
    }

    /// A cubic curve through two control points.
    #[allow(
        clippy::too_many_arguments,
        reason = "a cubic curve is six numbers; the builder takes them as SVG's `C` does"
    )]
    pub fn cubic_to(
        &mut self,
        first_x: f32,
        first_y: f32,
        second_x: f32,
        second_y: f32,
        x: f32,
        y: f32,
    ) -> &mut PathBuilder {
        if self.accept(&[first_x, first_y, second_x, second_y, x, y]) {
            self.commands.cubic_to(
                Point::new(first_x, first_y),
                Point::new(second_x, second_y),
                Point::new(x, y),
            );
        }
        self
    }

    /// SVG's elliptical arc to `(x, y)`: the radii, the ellipse's rotation in
    /// degrees, whether to take the larger of the two arcs, and whether to
    /// sweep in the positive-angle direction (clockwise on a screen).
    #[allow(
        clippy::too_many_arguments,
        reason = "an arc is SVG's seven arguments, in SVG's order"
    )]
    pub fn arc_to(
        &mut self,
        radius_x: f32,
        radius_y: f32,
        rotation_degrees: f32,
        large: bool,
        sweep: bool,
        x: f32,
        y: f32,
    ) -> &mut PathBuilder {
        if self.accept(&[radius_x, radius_y, rotation_degrees, x, y]) {
            self.commands.arc_to(
                radius_x,
                radius_y,
                rotation_degrees,
                large,
                sweep,
                Point::new(x, y),
            );
        }
        self
    }

    pub fn close(&mut self) -> &mut PathBuilder {
        self.segments_given += 1;
        if self.not_finite.is_none() {
            self.commands.close();
        }
        self
    }

    /// The path of every segment so far. The builder keeps them: more
    /// segments extend it, and a later `finish` answers the longer path.
    pub fn finish(&self) -> Result<Path, PathError> {
        if let Some(segment) = self.not_finite {
            return Err(PathError::NotFinite { segment });
        }
        Ok(Path::from_segments(self.commands.segments.clone()))
    }
}

/// A mask's key in the atlas (R1's "key variant"): the path's content hash,
/// the paint and the device scale, with the commands compared on a hit.
#[derive(Clone, Debug)]
pub(crate) struct PathMaskKey {
    pub(crate) path: Path,
    pub(crate) paint: PathPaint,
    paint_key: PaintKey,
    /// Device pixels per path unit, as bits.
    scale_bits: u32,
}

impl PathMaskKey {
    pub(crate) fn new(path: &Path, paint: PathPaint, device_scale: f32) -> PathMaskKey {
        PathMaskKey {
            path: path.clone(),
            paint,
            paint_key: paint.key(),
            scale_bits: device_scale.to_bits(),
        }
    }

    pub(crate) fn device_scale(&self) -> f32 {
        f32::from_bits(self.scale_bits)
    }

    /// The fields that order keys for eviction's tie break.
    pub(crate) fn order_fields(&self) -> (u64, PaintKey, u32) {
        (self.path.data.hash, self.paint_key, self.scale_bits)
    }
}

impl PartialEq for PathMaskKey {
    fn eq(&self, other: &PathMaskKey) -> bool {
        self.scale_bits == other.scale_bits
            && self.paint_key == other.paint_key
            && self.path.same_shape(&other.path)
    }
}

impl Eq for PathMaskKey {}

impl std::hash::Hash for PathMaskKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.path.data.hash.hash(state);
        self.paint_key.hash(state);
        self.scale_bits.hash(state);
    }
}

/// A path's mask: coverage bytes, and where its top-left pixel sits from the
/// path's origin, in device pixels (`top` DOWN, unlike a glyph's).
#[derive(Debug)]
pub(crate) struct RasterizedMask {
    pub(crate) left: i32,
    pub(crate) top: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

/// What rasterising a mask came to.
#[derive(Debug)]
pub(crate) enum Rasterized {
    Mask(RasterizedMask),
    /// No pixels: an empty path, a zero-width stroke, a fill of no area.
    Empty,
    /// Wider or taller than `maximum_side`: refused before any pixel is
    /// made.
    TooLarge,
}

/// Rasterises `key`'s path at its device scale: filled by its rule, or its
/// kept stroke outline filled non-zero.
pub(crate) fn rasterize(
    key: &PathMaskKey,
    scratch: &mut zeno::Scratch,
    maximum_side: u32,
) -> Rasterized {
    let path = &key.path;
    if path.is_empty() {
        return Rasterized::Empty;
    }
    let scale = key.device_scale();
    let outline;
    let (segments, rule): (&[Segment], zeno::Fill) = match key.paint {
        PathPaint::Fill(FillRule::NonZero) => (&path.data.segments, zeno::Fill::NonZero),
        PathPaint::Fill(FillRule::EvenOdd) => (&path.data.segments, zeno::Fill::EvenOdd),
        PathPaint::Stroke(stroke) => {
            if stroke.width <= 0.0 {
                return Rasterized::Empty;
            }
            outline = path.outline(&stroke);
            (&outline, zeno::Fill::NonZero)
        }
    };
    let mut mask = zeno::Mask::with_scratch(segments, scratch);
    mask.style(rule)
        .transform(Some(zeno::Transform::scale(scale, scale)));
    let mut size = (0, 0);
    mask.inspect(|_, width, height| size = (width, height));
    let (width, height) = size;
    if width == 0 || height == 0 {
        return Rasterized::Empty;
    }
    if width > maximum_side || height > maximum_side {
        return Rasterized::TooLarge;
    }
    let (pixels, placement) = mask.render();
    if pixels.iter().all(|coverage| *coverage == 0) {
        return Rasterized::Empty;
    }
    Rasterized::Mask(RasterizedMask {
        left: placement.left,
        top: placement.top,
        width: placement.width,
        height: placement.height,
        pixels,
    })
}

/// The paper's seven icons, shared with the golden images and the
/// measuring example.
#[cfg(test)]
#[path = "../../tests/support/icons.rs"]
pub(crate) mod tests_support;

#[cfg(test)]
mod tests;
