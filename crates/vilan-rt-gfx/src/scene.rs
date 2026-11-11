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
}

/// A built scene. See the module documentation.
#[derive(Clone, Debug)]
pub struct Scene {
    pub(crate) commands: Arc<[Command]>,
    primitive_count: usize,
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
        }
    }

    /// Primitives in the scene, those of nested scenes included (a text run
    /// counts once, whatever its glyph count).
    pub fn primitive_count(&self) -> usize {
        self.primitive_count
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

    fn primitive(&mut self, command: Command) {
        self.primitive_count += 1;
        self.commands.push(command);
    }

    pub fn rectangle(&mut self, rectangle: Rectangle) {
        self.primitive(Command::Rectangle(rectangle));
    }

    pub fn shadow(&mut self, shadow: Shadow) {
        self.primitive(Command::Shadow(shadow));
    }

    /// Draws a shaped paragraph with its top-left corner at `origin`.
    pub fn text(&mut self, layout: &TextLayout, origin: Point, color: Color) {
        self.primitive(Command::Text {
            layout: layout.clone(),
            origin,
            color,
        });
    }

    pub fn underline(&mut self, underline: Underline) {
        self.primitive(Command::Underline(underline));
    }

    /// Draws `image` stretched over `bounds`, clipped to `corner_radii`.
    pub fn image(&mut self, image: &Image, bounds: Rect, corner_radii: Corners) {
        self.primitive(Command::Image {
            image: image.clone(),
            bounds,
            corner_radii,
        });
    }

    /// Clips what follows, until the matching `pop`, to a rounded rectangle.
    /// Nested clips intersect.
    pub fn push_clip(&mut self, bounds: Rect, corner_radii: Corners) {
        self.open_groups += 1;
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
        self.open_groups += 1;
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
        self.open_groups += 1;
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
        self.commands.push(Command::Pop);
    }

    /// Appends a built scene, drawn under the groups open here.
    pub fn scene(&mut self, scene: &Scene) {
        if scene.commands.is_empty() {
            return;
        }
        self.primitive_count += scene.primitive_count;
        self.commands.push(Command::Scene(scene.clone()));
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
