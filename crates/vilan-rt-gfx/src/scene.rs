//! The scene: primitives in paint order, in dp, built once and drawn any
//! number of times.
//!
//! A [`Scene`] is immutable and cheap to clone (its commands are shared). It is
//! made with a [`SceneBuilder`]: primitives are appended in paint order, and
//! `push_clip`, `push_transform` and `push_layer` open a group that applies to
//! everything appended until the matching [`pop`](SceneBuilder::pop). A built
//! scene can be appended to another builder ([`SceneBuilder::scene`]), where
//! it draws under the clip, transform and opacity in force at that point.
//! That nesting is the retained-scene mechanism the crate documentation
//! describes.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::GfxError;
use crate::color::{Color, Fill};
use crate::geometry::{Corners, Edges, Point, Rect, Transform};
use crate::path::{Path, PathPaint, Stroke};
use crate::text::TextLayout;

/// A rectangle: per-corner radius, a fill, and a border with a width per side.
/// The border is drawn INSIDE the bounds, so a bordered and an unbordered
/// rectangle with the same bounds take the same space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rectangle {
    pub bounds: Rect,
    pub corner_radii: Corners,
    pub fill: Fill,
    pub border: Border,
}

impl Rectangle {
    /// A square-cornered, borderless rectangle.
    pub fn new(bounds: Rect, fill: impl Into<Fill>) -> Rectangle {
        Rectangle {
            bounds,
            corner_radii: Corners::ZERO,
            fill: fill.into(),
            border: Border::NONE,
        }
    }

    /// The same radius at every corner.
    pub fn with_radius(self, radius: f32) -> Rectangle {
        Rectangle {
            corner_radii: Corners::all(radius),
            ..self
        }
    }

    pub fn with_corner_radii(self, corner_radii: Corners) -> Rectangle {
        Rectangle {
            corner_radii,
            ..self
        }
    }

    pub fn with_border(self, border: Border) -> Rectangle {
        Rectangle { border, ..self }
    }
}

/// Border widths in dp per side, and the border's colour. A width under 2 dp
/// is a hairline: one device pixel at 100%, 125% and 150% (R16).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    pub widths: Edges,
    pub color: Color,
}

impl Border {
    pub const NONE: Border = Border {
        widths: Edges::ZERO,
        color: Color::TRANSPARENT,
    };

    /// The same width on every side.
    pub fn all(width: f32, color: Color) -> Border {
        Border {
            widths: Edges::all(width),
            color,
        }
    }
}

/// A blurred rounded rectangle, computed analytically in the shader (no
/// offscreen pass). Offset and spread are the caller's: move or grow
/// `bounds`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub bounds: Rect,
    pub corner_radius: f32,
    /// The blur radius in dp, as CSS's `box-shadow` blur: the Gaussian's
    /// standard deviation is half of it.
    pub blur: f32,
    pub color: Color,
}

/// A straight line under text, `thickness` dp tall with its top at
/// `origin.y`. The thickness follows the hairline rule, like a border.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Underline {
    pub origin: Point,
    pub width: f32,
    pub thickness: f32,
    pub color: Color,
}

/// Pixels to draw: RGBA, 8 bits per channel, sRGB with straight alpha, rows
/// top to bottom. Cheap to clone; the renderer uploads it once, on the first
/// frame that draws it, and frees the GPU copy after the last clone is
/// dropped.
///
/// The pixels stay with the `Image` (several renderers may each upload it);
/// decoding (PNG, JPEG) is `std::image`'s job, not this crate's.
#[derive(Clone, Debug)]
pub struct Image {
    pub(crate) data: Arc<ImageData>,
}

#[derive(Debug)]
pub(crate) struct ImageData {
    pub(crate) id: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

impl Image {
    /// Refuses a zero side or a pixel buffer that is not `width * height * 4`
    /// bytes.
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Result<Image, GfxError> {
        let expected = u64::from(width) * u64::from(height) * 4;
        if width == 0 || height == 0 || pixels.len() as u64 != expected {
            return Err(GfxError::InvalidImage {
                width,
                height,
                bytes: pixels.len(),
            });
        }
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Ok(Image {
            data: Arc::new(ImageData {
                id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
                width,
                height,
                pixels,
            }),
        })
    }

    pub fn width(&self) -> u32 {
        self.data.width
    }

    pub fn height(&self) -> u32 {
        self.data.height
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Command {
    Rectangle(Rectangle),
    Shadow(Shadow),
    Text {
        layout: TextLayout,
        origin: Point,
        color: Color,
    },
    Underline(Underline),
    Image {
        image: Image,
        bounds: Rect,
        corner_radii: Corners,
    },
    /// A path with its origin at `origin`, `scale` dp to one path unit (see
    /// [`SceneBuilder::path`]).
    Path {
        path: Path,
        origin: Point,
        scale: f32,
        paint: PathPaint,
        color: Color,
    },
    PushClip {
        bounds: Rect,
        corner_radii: Corners,
    },
    PushTransform(Transform),
    PushLayer {
        opacity: f32,
    },
    Pop,
    Scene(Scene),
    /// A nested scene moved by `offset` dp, which the encoder lands on the
    /// device grid (see [`SceneBuilder::scene_at`]).
    PlacedScene {
        scene: Scene,
        offset: Point,
    },
}

/// A built scene. See the module documentation.
#[derive(Clone, Debug)]
pub struct Scene {
    pub(crate) commands: Arc<[Command]>,
    primitive_count: usize,
    /// A rectangle, in the scene's own dp, that every pixel it can draw lies
    /// inside (`None`: it draws nothing). CONSERVATIVE, never tight: a text
    /// run counts its layout box grown by its height on every side (glyph ink
    /// may leave the line box), a shadow its blur's whole reach. The encoder
    /// skips a nested scene whose bounds miss the clip in force, which is
    /// what keeps a long list in a scrolled viewport from costing its whole
    /// length every frame.
    pub(crate) bounds: Option<Rect>,
}

impl Default for Scene {
    fn default() -> Self {
        Scene::empty()
    }
}

impl Scene {
    pub fn empty() -> Scene {
        Scene {
            commands: Arc::from(Vec::new()),
            primitive_count: 0,
            bounds: None,
        }
    }

    /// Primitives in the scene, those of nested scenes included (a text run
    /// counts once, whatever its glyph count).
    pub fn primitive_count(&self) -> usize {
        self.primitive_count
    }

    /// The rectangle every pixel of the scene lies inside, in its own dp (see
    /// the field); `None` for a scene that draws nothing.
    pub fn bounds(&self) -> Option<Rect> {
        self.bounds
    }

    /// Whether `self` and `other` are the same built scene (not merely equal
    /// content): how a cache above this crate knows a retained scene is
    /// unchanged.
    pub fn same_as(&self, other: &Scene) -> bool {
        Arc::ptr_eq(&self.commands, &other.commands)
    }
}

/// Why a scene could not be built.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SceneError {
    /// `pop` with no open group.
    PopWithoutPush,
    /// `finish` with groups still open.
    UnclosedGroups(usize),
    /// A transform with a scale that is not finite and positive, or a
    /// translation that is not finite.
    InvalidTransform(Transform),
    /// A layer opacity that is NaN. (Others are clamped into 0..=1.)
    InvalidOpacity,
    /// A path drawn at a scale that is not finite and positive, or at an
    /// origin that is not finite.
    InvalidPathPlacement { origin: Point, scale: f32 },
    /// A stroke whose width is negative or not finite, or whose miter limit
    /// is under 1 or not finite.
    InvalidStroke(Stroke),
}

impl std::fmt::Display for SceneError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SceneError::PopWithoutPush => {
                write!(formatter, "`pop` with no clip, transform or layer open")
            }
            SceneError::UnclosedGroups(count) => {
                write!(
                    formatter,
                    "{count} clip, transform or layer group(s) never popped"
                )
            }
            SceneError::InvalidTransform(transform) => write!(
                formatter,
                "a transform needs a finite translation and a finite, positive scale; got {transform:?}"
            ),
            SceneError::InvalidOpacity => write!(formatter, "a layer's opacity is NaN"),
            SceneError::InvalidPathPlacement { origin, scale } => write!(
                formatter,
                "a path needs a finite origin and a finite, positive scale; got the origin \
                 ({}, {}) and the scale {scale}",
                origin.x, origin.y
            ),
            SceneError::InvalidStroke(stroke) => write!(
                formatter,
                "a stroke needs a finite width of zero or more and a finite miter limit of at \
                 least 1; got the width {} and the miter limit {}",
                stroke.width, stroke.miter_limit
            ),
        }
    }
}

impl std::error::Error for SceneError {}

/// Builds a [`Scene`]. Misuse (an unmatched `pop`, an invalid transform) is
/// recorded and reported once, by [`finish`](SceneBuilder::finish), so a
/// builder driven by generated code needs no error check per call.
#[derive(Default)]
pub struct SceneBuilder {
    commands: Vec<Command>,
    open_groups: usize,
    primitive_count: usize,
    error: Option<SceneError>,
    /// The transform in force for the bounds (the identity when `None`), and
    /// the one each open group restores at its `pop`.
    transform: Option<Transform>,
    transforms: Vec<Option<Transform>>,
    bounds: Option<Rect>,
}

/// The smallest rectangle holding both.
pub(crate) fn union(first: Option<Rect>, second: Rect) -> Option<Rect> {
    if !(second.size.width >= 0.0 && second.size.height >= 0.0) {
        return first;
    }
    Some(match first {
        None => second,
        Some(first) => {
            let left = first.left().min(second.left());
            let top = first.top().min(second.top());
            let right = first.right().max(second.right());
            let bottom = first.bottom().max(second.bottom());
            Rect::new(left, top, right - left, bottom - top)
        }
    })
}

fn grown(rect: Rect, by: f32) -> Rect {
    Rect::new(
        rect.left() - by,
        rect.top() - by,
        rect.size.width + 2.0 * by,
        rect.size.height + 2.0 * by,
    )
}

impl SceneBuilder {
    pub fn new() -> SceneBuilder {
        SceneBuilder::default()
    }

    fn record(&mut self, error: SceneError) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    fn primitive(&mut self, command: Command, extent: Rect) {
        self.primitive_count += 1;
        self.commands.push(command);
        self.include(extent);
    }

    /// Grows the bounds by `extent`, in the coordinates of the groups open.
    fn include(&mut self, extent: Rect) {
        let extent = match self.transform {
            Some(transform) => transform.apply_rect(extent),
            None => extent,
        };
        self.bounds = union(self.bounds, extent);
    }

    pub fn rectangle(&mut self, rectangle: Rectangle) {
        let extent = rectangle.bounds;
        self.primitive(Command::Rectangle(rectangle), extent);
    }

    pub fn shadow(&mut self, shadow: Shadow) {
        // The Gaussian's standard deviation is half the blur; three of them
        // is where it has faded out.
        let extent = grown(shadow.bounds, 1.5 * shadow.blur.max(0.0) + 1.0);
        self.primitive(Command::Shadow(shadow), extent);
    }

    /// Draws a shaped paragraph with its top-left corner at `origin`.
    pub fn text(&mut self, layout: &TextLayout, origin: Point, color: Color) {
        let size = layout.size();
        // Glyph ink may leave the line box (an italic overhang, a tall
        // accent): its whole height again on every side covers it.
        let extent = grown(
            Rect::new(origin.x, origin.y, size.width, size.height),
            size.height.max(1.0),
        );
        self.primitive(
            Command::Text {
                layout: layout.clone(),
                origin,
                color,
            },
            extent,
        );
    }

    pub fn underline(&mut self, underline: Underline) {
        let extent = grown(
            Rect::new(
                underline.origin.x,
                underline.origin.y,
                underline.width,
                underline.thickness,
            ),
            1.0,
        );
        self.primitive(Command::Underline(underline), extent);
    }

    /// Draws `image` stretched over `bounds`, clipped to `corner_radii`.
    pub fn image(&mut self, image: &Image, bounds: Rect, corner_radii: Corners) {
        self.primitive(
            Command::Image {
                image: image.clone(),
                bounds,
                corner_radii,
            },
            bounds,
        );
    }

    /// Draws `path` with its origin at `origin` dp and `scale` dp to one
    /// path unit, painted solid `color` (gfx-path.md 2.1): filled by a rule,
    /// or stroked with a width in the path's units, which scales with it.
    /// The renderer snaps the origin to a whole device pixel (R7) and draws
    /// a coverage mask made at the device size (`path/mod.rs`).
    pub fn path(&mut self, path: &Path, origin: Point, scale: f32, paint: PathPaint, color: Color) {
        if !(origin.x.is_finite() && origin.y.is_finite() && scale.is_finite() && scale > 0.0) {
            self.record(SceneError::InvalidPathPlacement { origin, scale });
            return;
        }
        if let PathPaint::Stroke(stroke) = paint
            && !stroke.is_valid()
        {
            self.record(SceneError::InvalidStroke(stroke));
            return;
        }
        // The path's points (control points included) bound its ink, grown
        // by the stroke's reach, and by one dp for the origin's snap.
        let bounds = path.bounds();
        let reach = paint.reach();
        let extent = grown(
            Rect::new(
                origin.x + (bounds.left() - reach) * scale,
                origin.y + (bounds.top() - reach) * scale,
                (bounds.size.width + 2.0 * reach) * scale,
                (bounds.size.height + 2.0 * reach) * scale,
            ),
            1.0,
        );
        self.primitive(
            Command::Path {
                path: path.clone(),
                origin,
                scale,
                paint,
                color,
            },
            extent,
        );
    }

    fn open_group(&mut self, transform: Option<Transform>) {
        self.open_groups += 1;
        self.transforms.push(self.transform);
        if transform.is_some() {
            self.transform = transform;
        }
    }

    /// Clips what follows, until the matching `pop`, to a rounded rectangle.
    /// Nested clips intersect.
    pub fn push_clip(&mut self, bounds: Rect, corner_radii: Corners) {
        self.open_group(None);
        self.commands.push(Command::PushClip {
            bounds,
            corner_radii,
        });
    }

    /// Transforms what follows, until the matching `pop`, inside the
    /// transform already in force.
    pub fn push_transform(&mut self, transform: Transform) {
        if !transform.is_valid() {
            self.record(SceneError::InvalidTransform(transform));
        }
        let combined = self
            .transform
            .unwrap_or(Transform::IDENTITY)
            .then(transform);
        self.open_group(Some(combined));
        self.commands.push(Command::PushTransform(transform));
    }

    /// Multiplies the opacity of what follows, until the matching `pop`, by
    /// `opacity` (clamped into 0..=1).
    ///
    /// Applied per primitive, as GPUI does: where two primitives inside one
    /// translucent layer overlap, the lower one shows through the upper. A
    /// layer that composites as one image needs an offscreen pass, which the
    /// first version does not take (see the crate documentation).
    pub fn push_layer(&mut self, opacity: f32) {
        if opacity.is_nan() {
            self.record(SceneError::InvalidOpacity);
        }
        self.open_group(None);
        self.commands.push(Command::PushLayer {
            opacity: if opacity.is_nan() {
                1.0
            } else {
                opacity.clamp(0.0, 1.0)
            },
        });
    }

    /// Closes the most recently opened clip, transform or layer.
    pub fn pop(&mut self) {
        if self.open_groups == 0 {
            self.record(SceneError::PopWithoutPush);
            return;
        }
        self.open_groups -= 1;
        self.transform = self.transforms.pop().flatten();
        self.commands.push(Command::Pop);
    }

    /// Appends a built scene, drawn under the groups open here.
    pub fn scene(&mut self, scene: &Scene) {
        if scene.commands.is_empty() {
            return;
        }
        self.primitive_count += scene.primitive_count;
        self.commands.push(Command::Scene(scene.clone()));
        if let Some(bounds) = scene.bounds {
            self.include(bounds);
        }
    }

    /// Appends a built scene moved by `offset` dp, ROUNDED TO WHOLE DEVICE
    /// PIXELS when it is drawn: how `std::ui` places each element's scene,
    /// which is built in the element's own coordinates. Because the move is
    /// a whole number of pixels, every pixel the scene draws (a snapped edge,
    /// a glyph's subpixel position, a rounded baseline) is the one it draws
    /// at offset zero, moved; an offset already on the grid (a snapped box's
    /// corner) draws exactly where it says. (The bounds count the offset
    /// unrounded: half a device pixel off at most, inside the encoder's
    /// one-pixel margin for culling.)
    pub fn scene_at(&mut self, scene: &Scene, offset: Point) {
        if scene.commands.is_empty() {
            return;
        }
        if !(offset.x.is_finite() && offset.y.is_finite()) {
            self.record(SceneError::InvalidTransform(Transform::translate(
                offset.x, offset.y,
            )));
            return;
        }
        self.primitive_count += scene.primitive_count;
        self.commands.push(Command::PlacedScene {
            scene: scene.clone(),
            offset,
        });
        if let Some(bounds) = scene.bounds {
            self.include(Rect::new(
                bounds.left() + offset.x,
                bounds.top() + offset.y,
                bounds.size.width,
                bounds.size.height,
            ));
        }
    }

    pub fn finish(self) -> Result<Scene, SceneError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.open_groups > 0 {
            return Err(SceneError::UnclosedGroups(self.open_groups));
        }
        Ok(Scene {
            commands: Arc::from(self.commands),
            primitive_count: self.primitive_count,
            bounds: self.bounds,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unmatched_pop_is_reported_by_finish() {
        let mut builder = SceneBuilder::new();
        builder.pop();
        assert_eq!(builder.finish().err(), Some(SceneError::PopWithoutPush));
    }

    #[test]
    fn an_unclosed_group_is_reported_by_finish() {
        let mut builder = SceneBuilder::new();
        builder.push_layer(0.5);
        builder.push_clip(Rect::new(0.0, 0.0, 1.0, 1.0), Corners::ZERO);
        builder.pop();
        assert_eq!(builder.finish().err(), Some(SceneError::UnclosedGroups(1)));
    }

    #[test]
    fn a_zero_scale_transform_is_refused() {
        let mut builder = SceneBuilder::new();
        builder.push_transform(Transform::scale(0.0));
        builder.pop();
        assert!(matches!(
            builder.finish(),
            Err(SceneError::InvalidTransform(_))
        ));
    }

    #[test]
    fn a_nested_scene_counts_its_primitives_and_keeps_its_identity() {
        let mut inner = SceneBuilder::new();
        inner.rectangle(Rectangle::new(Rect::new(0.0, 0.0, 4.0, 4.0), Color::BLACK));
        inner.rectangle(Rectangle::new(Rect::new(8.0, 0.0, 4.0, 4.0), Color::BLACK));
        let inner = inner.finish().expect("balanced");
        let mut outer = SceneBuilder::new();
        outer.push_transform(Transform::translate(10.0, 10.0));
        outer.scene(&inner);
        outer.pop();
        outer.scene(&inner);
        let outer = outer.finish().expect("balanced");
        assert_eq!(outer.primitive_count(), 4);
        assert!(inner.same_as(&inner.clone()));
        assert!(!inner.same_as(&outer));
    }

    #[test]
    fn an_image_must_be_exactly_its_pixels() {
        assert!(Image::from_rgba(2, 2, vec![0; 16]).is_ok());
        assert!(Image::from_rgba(2, 2, vec![0; 15]).is_err());
        assert!(Image::from_rgba(0, 2, Vec::new()).is_err());
    }
}
