//! The seven icons of the path paper's spike (gfx-path.md, branch `path-58`),
//! shared by the crate's path unit tests, its golden images and
//! `examples/measure.rs` (each includes this file by path).
//!
//! Typed from Lucide's published shapes (ISC licence) for the check, the
//! circled check, the star, the gear, the chevron and the heart; the ring is
//! made up to give a filled shape with an even-odd hole. Lucide's `<circle>`
//! elements are written as two arcs, which is what an icon generator emits.
//! Every icon is in a 24-unit box; the stroked ones are Lucide's: 2 units
//! wide, round caps, round joins.

#![allow(dead_code, reason = "each includer uses its own part of the icons")]

/// How an icon is painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconPaint {
    /// Lucide's stroke: [`STROKE_WIDTH`] units, round caps and joins.
    Stroke,
    Fill,
    FillEvenOdd,
}

pub struct Icon {
    pub name: &'static str,
    pub data: &'static str,
    pub paint: IconPaint,
}

/// The icons' box, in path units.
pub const ICON_BOX: f32 = 24.0;

/// Lucide's stroke width, in path units.
pub const STROKE_WIDTH: f32 = 2.0;

/// The sizes the paper measured, in dp.
pub const SIZES: [f32; 4] = [16.0, 24.0, 48.0, 256.0];

/// The ring: two circles winding the same way, so non-zero fills the hole
/// and even-odd does not.
pub const RING: &str =
    "M12 2a10 10 0 1 1 0 20a10 10 0 1 1 0-20ZM12 7a5 5 0 1 1 0 10a5 5 0 1 1 0-10Z";

pub const ICONS: &[Icon] = &[
    Icon {
        name: "check",
        data: "M20 6 9 17l-5-5",
        paint: IconPaint::Stroke,
    },
    Icon {
        name: "circle-check",
        data: "M22 12a10 10 0 1 1-20 0a10 10 0 1 1 20 0ZM9 12l2 2 4-4",
        paint: IconPaint::Stroke,
    },
    Icon {
        name: "star",
        data: "M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z",
        paint: IconPaint::Stroke,
    },
    Icon {
        name: "settings",
        data: "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2zM15 12a3 3 0 1 1-6 0a3 3 0 1 1 6 0Z",
        paint: IconPaint::Stroke,
    },
    Icon {
        name: "chevron-down",
        data: "m6 9 6 6 6-6",
        paint: IconPaint::Stroke,
    },
    Icon {
        name: "heart (filled)",
        data: "M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z",
        paint: IconPaint::Fill,
    },
    Icon {
        name: "ring (filled, even-odd hole)",
        data: RING,
        paint: IconPaint::FillEvenOdd,
    },
];
