//! Positions, sizes and rectangles in display pixels (dp), and the transform a
//! scene applies to them.
//!
//! Every length a caller hands this crate is in dp (native-ui.md section 5):
//! the 8 of a design measured in pixels. Device pixels appear only inside the
//! renderer, after [`Units`](crate::Units) has scaled and snapped them.

/// A point in dp.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Point {
        Point { x, y }
    }
}

/// A width and a height in dp.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const fn new(width: f32, height: f32) -> Size {
        Size { width, height }
    }
}

/// An axis-aligned rectangle in dp: its top-left corner and its size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect {
            origin: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    pub fn left(&self) -> f32 {
        self.origin.x
    }

    pub fn top(&self) -> f32 {
        self.origin.y
    }

    pub fn right(&self) -> f32 {
        self.origin.x + self.size.width
    }

    pub fn bottom(&self) -> f32 {
        self.origin.y + self.size.height
    }

    /// Whether the rectangle encloses no area (a zero or negative side, or a
    /// NaN): such a primitive draws nothing and is dropped before ordering.
    pub fn is_empty(&self) -> bool {
        !(self.size.width > 0.0 && self.size.height > 0.0)
    }
}

/// One value per corner, clockwise from the top left. Used for corner radii.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corners {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl Corners {
    pub const ZERO: Corners = Corners::all(0.0);

    /// The same value at every corner.
    pub const fn all(value: f32) -> Corners {
        Corners {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }

    pub(crate) fn to_array(self) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }
}

/// One value per side, clockwise from the top. Used for border widths.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    pub const ZERO: Edges = Edges::all(0.0);

    /// The same value on every side.
    pub const fn all(value: f32) -> Edges {
        Edges {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

/// A translation and a UNIFORM scale, applied as `point * scale + translation`.
///
/// Uniform on purpose (native-ui.md 3.1: "translate and scale first; rotation
/// later"): a uniform scale maps a rounded rectangle to a rounded rectangle and
/// a clip to a clip, so every primitive stays one quad with its shape still
/// computed exactly in the shader, and text under it is rasterised at the
/// scaled size rather than stretched. A non-uniform scale or a rotation would
/// break all three, and arrives with its own design.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: Point,
    pub scale: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Transform::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        translation: Point::ZERO,
        scale: 1.0,
    };

    pub const fn translate(x: f32, y: f32) -> Transform {
        Transform {
            translation: Point::new(x, y),
            scale: 1.0,
        }
    }

    pub const fn scale(scale: f32) -> Transform {
        Transform {
            translation: Point::ZERO,
            scale,
        }
    }

    /// `self` applied after `inner`: a point goes through `inner` first. This is
    /// how a pushed transform combines with the one already in force.
    pub fn then(self, inner: Transform) -> Transform {
        Transform {
            translation: self.apply(inner.translation),
            scale: self.scale * inner.scale,
        }
    }

    pub fn apply(&self, point: Point) -> Point {
        Point::new(
            point.x * self.scale + self.translation.x,
            point.y * self.scale + self.translation.y,
        )
    }

    pub(crate) fn apply_rect(&self, rect: Rect) -> Rect {
        let origin = self.apply(rect.origin);
        Rect {
            origin,
            size: Size::new(rect.size.width * self.scale, rect.size.height * self.scale),
        }
    }

    /// Whether this transform can be applied: a finite translation and a
    /// finite, positive scale. A zero or negative scale would mirror or
    /// collapse geometry the shader assumes is upright.
    pub(crate) fn is_valid(&self) -> bool {
        self.translation.x.is_finite()
            && self.translation.y.is_finite()
            && self.scale.is_finite()
            && self.scale > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pushed_transform_applies_inside_the_one_in_force() {
        let outer = Transform::translate(10.0, 20.0);
        let inner = Transform::scale(2.0);
        let combined = outer.then(inner);
        // inner first (x2), then outer (+10, +20)
        assert_eq!(combined.apply(Point::new(1.0, 1.0)), Point::new(12.0, 22.0));
        let reversed = inner.then(outer);
        assert_eq!(reversed.apply(Point::new(1.0, 1.0)), Point::new(22.0, 42.0));
    }

    #[test]
    fn an_empty_rect_is_one_with_no_area() {
        assert!(Rect::new(0.0, 0.0, 0.0, 5.0).is_empty());
        assert!(Rect::new(0.0, 0.0, 5.0, -1.0).is_empty());
        assert!(Rect::new(0.0, 0.0, f32::NAN, 1.0).is_empty());
        assert!(!Rect::new(0.0, 0.0, 0.5, 0.5).is_empty());
    }
}
