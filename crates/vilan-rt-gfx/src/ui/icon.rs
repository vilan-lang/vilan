//! The `icon` element (gfx-path.md S3, rulings R9 and R10): a path in a
//! square box, drawn as `std::gfx` draws any path, in the colour and at the
//! size of the text around it.
//!
//! # The rules
//!
//! - **Shape.** An icon draws an [`IconShape`]: a [`Path`] in a square box
//!   of `view_box` path units (24 for Lucide), filled by a rule or stroked
//!   with a width in path units, scaled so that box is the icon's SIDE
//!   (`scale = side / view_box`), from its content box's top-left corner.
//!   Its pixels are the path primitive's: a coverage mask at the device
//!   size, the origin snapped to a whole device pixel (R7), the mask shared
//!   with every other path of the same content.
//! - **Colour (R9).** The inherited TEXT colour, as SVG's `currentColor`:
//!   the icon's resolved text colour, so a colour set on an ancestor (or on
//!   the icon itself, which is what `std::ui`'s `color(..)` does) reaches
//!   it, and a later change of it (a hovered button's text turning red)
//!   repaints the icon with the text beside it, with no layout pass.
//! - **Size (R9).** Square: its side is `size`, in dp, or else the
//!   inherited text size, so an icon beside a label matches it, and a change
//!   of that size lays the icon out again. Its box measures as that square.
//!   A box made larger than its side — stretched across a row or a column,
//!   or given a width or a height — draws the icon as a text draws its
//!   glyphs: at its side, from the box's top-left corner (a column of icons
//!   is not a column of growing icons; `size` sizes an icon).
//! - **A change of shape repaints the icon alone.** Its box does not depend
//!   on its shape, so a new shape (a chevron that flips, a check that comes
//!   and goes) rebuilds the icon's scene and the chain above it, and never
//!   runs the solver.
//! - **Accessibility (R10)** is `access.rs`'s: decorative, and left out,
//!   unless it has a label; labelled, an image named by it.

use super::{Content, NodeId, ResolvedText, UiError, UiTree};
use crate::geometry::Point;
use crate::path::{Path, PathPaint};
use crate::scene::SceneBuilder;

/// What an icon draws: a path in a square box `view_box` path units on a
/// side, from the path's origin, painted by `paint` (a stroke's width is in
/// path units, so it scales with the icon).
#[derive(Clone, Debug)]
pub struct IconShape {
    path: Path,
    view_box: f32,
    paint: PathPaint,
}

impl IconShape {
    /// A shape, or why it cannot be one: a box that is not a positive
    /// finite number, or a stroke whose width is negative or not finite or
    /// whose miter limit is under 1 (a width of 0 draws nothing).
    pub fn new(path: Path, view_box: f32, paint: PathPaint) -> Result<IconShape, UiError> {
        if !(view_box.is_finite() && view_box > 0.0) {
            return Err(UiError::InvalidIcon(format!(
                "an icon's box must be a positive number of path units, not {view_box}"
            )));
        }
        if let PathPaint::Stroke(stroke) = paint
            && !stroke.is_valid()
        {
            return Err(UiError::InvalidIcon(format!(
                "an icon's stroke must have a finite width of at least 0 and a miter limit of \
                 at least 1, not a width of {} and a limit of {}",
                stroke.width, stroke.miter_limit
            )));
        }
        Ok(IconShape {
            path,
            view_box,
            paint,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn view_box(&self) -> f32 {
        self.view_box
    }

    pub fn paint(&self) -> PathPaint {
        self.paint
    }

    /// The same drawing: the same path data (two parses of one string are
    /// the same shape), box and paint.
    fn draws_as(&self, other: &IconShape) -> bool {
        self.path.same_shape(&other.path)
            && self.view_box == other.view_box
            && self.paint == other.paint
    }
}

/// An icon element's content: its shape (none until `of` sets one: it
/// draws nothing, and still takes its size) and its size, when set.
#[derive(Clone, Debug, Default)]
pub(super) struct IconContent {
    shape: Option<IconShape>,
    size: Option<f32>,
}

impl IconContent {
    /// Its side in dp: its own size, or else the text size it inherits.
    fn side(&self, resolved: &ResolvedText) -> f32 {
        self.size.unwrap_or(resolved.style.size)
    }

    /// The solver's measure: a square of its side, where its parent has not
    /// decided a side of the box.
    pub(super) fn measure(
        &self,
        resolved: &ResolvedText,
        known: taffy::Size<Option<f32>>,
    ) -> taffy::Size<f32> {
        let side = self.side(resolved);
        taffy::Size {
            width: known.width.unwrap_or(side),
            height: known.height.unwrap_or(side),
        }
    }

    /// Draws the shape at its side from `origin` (the content box's
    /// top-left corner, in the element's own coordinates), in the resolved
    /// text colour.
    pub(super) fn paint(&self, builder: &mut SceneBuilder, origin: Point, resolved: &ResolvedText) {
        let Some(shape) = &self.shape else {
            return;
        };
        let side = self.side(resolved);
        if side <= 0.0 || resolved.color.alpha <= 0.0 {
            return;
        }
        builder.path(
            &shape.path,
            origin,
            side / shape.view_box,
            shape.paint,
            resolved.color,
        );
    }

    /// What a change of the inherited text style from `before` to `after`
    /// does to the icon: (lay it out again, repaint it). A new text size
    /// moves an icon that takes its size from it; a new colour repaints it.
    pub(super) fn follows(&self, before: &ResolvedText, after: &ResolvedText) -> (bool, bool) {
        let relayout = self.size.is_none() && before.style.size != after.style.size;
        let repaint = relayout || before.color != after.color;
        (relayout, repaint)
    }
}

impl UiTree {
    fn icon_content(
        &mut self,
        node: NodeId,
        property: &'static str,
    ) -> Result<&mut IconContent, UiError> {
        let kind = self.kind(node);
        match &mut self.data_mut(node).content {
            Content::Icon(content) => Ok(content),
            _ => Err(UiError::WrongKind { property, kind }),
        }
    }

    /// The shape an icon draws (`std::ui`'s `of`). Paint only: the icon's
    /// box does not depend on its shape, so its scene (and the chain above
    /// it) is rebuilt and the solver does not run. The same drawing again
    /// changes nothing.
    pub fn set_icon(&mut self, node: NodeId, shape: IconShape) -> Result<(), UiError> {
        let content = self.icon_content(node, "icon")?;
        if content
            .shape
            .as_ref()
            .is_some_and(|kept| kept.draws_as(&shape))
        {
            return Ok(());
        }
        content.shape = Some(shape);
        self.invalidate(node);
        Ok(())
    }

    /// An icon's side in dp (`std::ui`'s `size`), or `None` for the
    /// inherited text size. A finite number, at least 0 (0 takes no room and
    /// draws nothing). Lays the icon out again.
    pub fn set_icon_size(&mut self, node: NodeId, size: Option<f32>) -> Result<(), UiError> {
        if let Some(size) = size
            && !(size.is_finite() && size >= 0.0)
        {
            return Err(UiError::InvalidIcon(format!(
                "an icon's size must be a finite number of dp, at least 0, not {size}"
            )));
        }
        let content = self.icon_content(node, "size")?;
        if content.size == size {
            return Ok(());
        }
        content.size = size;
        self.taffy.mark_dirty(node).expect("an element");
        self.dirty.push(node);
        self.invalidate(node);
        self.layout_changed();
        Ok(())
    }

    /// An icon's shape, as last set.
    pub fn icon_shape(&self, node: NodeId) -> Option<&IconShape> {
        match &self.data(node).content {
            Content::Icon(content) => content.shape.as_ref(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
