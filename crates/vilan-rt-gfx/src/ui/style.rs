//! An element's style as LAYERS (native-ui.md section 4.3; part P3).
//!
//! What a vilan `Style` value carries crosses into an element as layers: one
//! unconditional layer per `style(..)` call, and one more per `when(..)`
//! condition it holds. Each `style(..)` call is a GROUP of layers, so a style
//! bound to a signal replaces exactly its own group when the signal writes,
//! and leaves the groups other calls made alone.
//!
//! The element's EFFECTIVE style is resolved from its layers: every
//! unconditional layer in the order they were set, then every conditional
//! layer whose condition holds, in the order they were set — a condition's
//! fields win over the unconditional fields whatever order the two were
//! composed in, and among conditions the later wins. A field no layer sets
//! is the element's default (no background, no border, square corners, no
//! shadow, opaque, its text style inherited).
//!
//! Conditions are TYPED and CLOSED: the element hovered (the pointer over it
//! or over an element inside it), pressed (the primary button went down on
//! it or inside it and has not come up), focused (it holds the focus), and
//! the window's width at least or below a number of dp. A condition that
//! names several holds when all of them do (a nested `when`).

use crate::color::Color;
use crate::geometry::{Corners, Edges};

use super::{DropShadow, Paint, TextOverrides};

/// What the pointer and the focus do to an element now: what the
/// conditions read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Interaction {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
}

/// When a conditional layer applies. Every part that is set must hold.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Condition {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    /// The window at least this many dp wide.
    pub window_at_least: Option<f32>,
    /// The window narrower than this many dp.
    pub window_below: Option<f32>,
}

impl Condition {
    /// Whether it holds for an element in `state`, in a window `width` dp
    /// wide (`None` before the first layout: no width condition holds).
    pub fn holds(&self, state: Interaction, width: Option<f32>) -> bool {
        (!self.hovered || state.hovered)
            && (!self.pressed || state.pressed)
            && (!self.focused || state.focused)
            && self
                .window_at_least
                .is_none_or(|at_least| width.is_some_and(|width| width >= at_least))
            && self
                .window_below
                .is_none_or(|below| width.is_some_and(|width| width < below))
    }

    pub(crate) fn reads_width(&self) -> bool {
        self.window_at_least.is_some() || self.window_below.is_some()
    }

    pub(crate) fn reads_interaction(&self) -> bool {
        self.hovered || self.pressed || self.focused
    }
}

/// The fields of a style layer: each `None` sets nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleFields {
    pub background: Option<Color>,
    /// Widths per side, and the colour.
    pub border: Option<(Edges, Color)>,
    pub corners: Option<Corners>,
    pub shadow: Option<DropShadow>,
    pub opacity: Option<f32>,
    /// The text fields, inherited by the elements inside.
    pub text: TextOverrides,
}

impl StyleFields {
    /// `other`'s fields where it sets them.
    fn overlay(&mut self, other: &StyleFields) {
        if other.background.is_some() {
            self.background = other.background;
        }
        if other.border.is_some() {
            self.border = other.border;
        }
        if other.corners.is_some() {
            self.corners = other.corners;
        }
        if other.shadow.is_some() {
            self.shadow = other.shadow;
        }
        if other.opacity.is_some() {
            self.opacity = other.opacity;
        }
        let text = &other.text;
        if text.family.is_some() {
            self.text.family = text.family.clone();
        }
        if text.size.is_some() {
            self.text.size = text.size;
        }
        if text.line_height.is_some() {
            self.text.line_height = text.line_height;
        }
        if text.weight.is_some() {
            self.text.weight = text.weight;
        }
        if text.italic.is_some() {
            self.text.italic = text.italic;
        }
        if text.color.is_some() {
            self.text.color = text.color;
        }
    }

    /// The paint these fields give, every unset field at its default.
    pub(crate) fn paint(&self) -> Paint {
        let default = Paint::default();
        let (border_widths, border_color) = self
            .border
            .unwrap_or((default.border_widths, default.border_color));
        Paint {
            background: self.background,
            border_widths,
            border_color,
            corners: self.corners.unwrap_or(default.corners),
            shadow: self.shadow,
            opacity: self.opacity.unwrap_or(default.opacity),
        }
    }
}

/// One layer of an element's style.
#[derive(Clone, Debug)]
pub(crate) struct Layer {
    /// Its id, unique on its element: what the host's setters name.
    pub(crate) id: u32,
    /// The `style(..)` call it came from.
    pub(crate) group: u32,
    pub(crate) condition: Option<Condition>,
    pub(crate) fields: StyleFields,
}

/// The group of the layer the tree's own setters (`set_background` and the
/// rest, which a Rust caller uses) write: the first, so every vilan
/// `style(..)` call is laid over it.
pub(crate) const DIRECT_GROUP: u32 = 0;

/// An element's layers, and the ids it hands out.
#[derive(Clone, Debug, Default)]
pub(crate) struct Layers {
    pub(crate) layers: Vec<Layer>,
    next_layer: u32,
    next_group: u32,
}

impl Layers {
    /// A new group, after every other.
    pub(crate) fn new_group(&mut self) -> u32 {
        self.next_group += 1;
        self.next_group
    }

    /// Takes every layer of `group` out (a bound style writing again).
    pub(crate) fn clear_group(&mut self, group: u32) {
        self.layers.retain(|layer| layer.group != group);
    }

    /// A new, empty layer of `group`: answers its id.
    pub(crate) fn add(&mut self, group: u32, condition: Option<Condition>) -> u32 {
        self.next_layer += 1;
        let id = self.next_layer;
        // A group's layers stay together, in the group order: a group
        // written again keeps its place among the others.
        let at = self
            .layers
            .iter()
            .position(|layer| layer.group > group)
            .unwrap_or(self.layers.len());
        self.layers.insert(
            at,
            Layer {
                id,
                group,
                condition,
                fields: StyleFields::default(),
            },
        );
        id
    }

    pub(crate) fn fields_mut(&mut self, id: u32) -> Option<&mut StyleFields> {
        self.layers
            .iter_mut()
            .find(|layer| layer.id == id)
            .map(|layer| &mut layer.fields)
    }

    /// The direct layer, made on first use.
    pub(crate) fn direct(&mut self) -> &mut StyleFields {
        let position = match self
            .layers
            .iter()
            .position(|layer| layer.group == DIRECT_GROUP)
        {
            Some(position) => position,
            None => {
                let id = self.add(DIRECT_GROUP, None);
                self.layers
                    .iter()
                    .position(|layer| layer.id == id)
                    .expect("just added")
            }
        };
        &mut self.layers[position].fields
    }

    pub(crate) fn reads_width(&self) -> bool {
        self.layers.iter().any(|layer| {
            layer
                .condition
                .is_some_and(|condition| condition.reads_width())
        })
    }

    pub(crate) fn reads_interaction(&self) -> bool {
        self.layers.iter().any(|layer| {
            layer
                .condition
                .is_some_and(|condition| condition.reads_interaction())
        })
    }

    /// The effective fields for an element in `state` in a window `width`
    /// dp wide: see the module documentation.
    pub(crate) fn resolve(&self, state: Interaction, width: Option<f32>) -> StyleFields {
        let mut fields = StyleFields::default();
        for layer in &self.layers {
            if layer.condition.is_none() {
                fields.overlay(&layer.fields);
            }
        }
        for layer in &self.layers {
            if let Some(condition) = layer.condition
                && condition.holds(state, width)
            {
                fields.overlay(&layer.fields);
            }
        }
        fields
    }
}
