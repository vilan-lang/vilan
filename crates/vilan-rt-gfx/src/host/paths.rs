//! `std::gfx`'s paths (gfx-path.md S2, its 2.2): the externs behind `Path`,
//! `PathBuilder` and `Frame::path`.
//!
//! A path and a path builder are host values the program holds (this crate's
//! [`Path`], and [`PathBuilder`] here, a shared handle on the crate's
//! builder); parsing crosses as a string in and a path or an error string
//! out; a builder grows by one call of numbers per segment, nothing allocated
//! per call; a draw is one call of numbers, as `__gfx_text` is.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use vilan_rt::{Js, Json, Str, panic_with, str_new};

use super::{color, with_frame};
use crate::geometry::Point;
use crate::path::{Cap, FillRule, Join, Path, PathPaint, Stroke};

/// A path being built, as the program holds it: a copy is the same builder
/// (every `PathBuilder` method of `std::gfx` answers the builder it was
/// called on), and two are `==` only when they are one.
#[derive(Clone)]
pub struct PathBuilder {
    builder: Rc<RefCell<crate::path::PathBuilder>>,
}

/// `__gfx_path_parse(data)` — `Path::from_svg`: the path, or why the data
/// is not one, naming the byte.
pub fn path_parse(data: &Str) -> Result<Path, Str> {
    Path::from_svg(data).map_err(|error| str_new(&error.to_string()))
}

/// `__gfx_path_builder()` — `Path::builder`: an empty builder.
pub fn path_builder() -> PathBuilder {
    PathBuilder {
        builder: Rc::new(RefCell::new(crate::path::PathBuilder::new())),
    }
}

/// `__gfx_path_segment(builder, verb, a, b, c, d, e, f, g)` — one segment:
/// `verb` 0 a move to `(a, b)`, 1 a line to `(a, b)`, 2 a quadratic curve
/// through `(a, b)` to `(c, d)`, 3 a cubic curve through `(a, b)` and
/// `(c, d)` to `(e, f)`, 4 an arc with radii `a`, `b`, rotation `c` degrees,
/// the large-arc flag `d` and the sweep flag `e` (each 0 or 1) to `(f, g)`,
/// 5 a close. Unused numbers are 0.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each: a segment crosses as \
              numbers so nothing is allocated per segment"
)]
pub fn path_segment(
    builder: &PathBuilder,
    verb: i32,
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
    g: f64,
) {
    let mut builder = builder.builder.borrow_mut();
    let [a, b, c, d, e, f, g] = [a, b, c, d, e, f, g].map(|number| number as f32);
    match verb {
        0 => builder.move_to(a, b),
        1 => builder.line_to(a, b),
        2 => builder.quad_to(a, b, c, d),
        3 => builder.cubic_to(a, b, c, d, e, f),
        4 => builder.arc_to(a, b, c, d != 0.0, e != 0.0, f, g),
        5 => builder.close(),
        _ => panic_with(&format!("{verb} is not a `std::gfx` path segment code")),
    };
}

/// `__gfx_path_finish(builder)` — `PathBuilder::finish`: the path of every
/// segment so far (the builder keeps them). A coordinate that was not a
/// finite number fails the program, naming the segment.
pub fn path_finish(builder: &PathBuilder) -> Path {
    builder
        .builder
        .borrow()
        .finish()
        .unwrap_or_else(|error| panic_with(&format!("`PathBuilder::finish`: {error}")))
}

/// `__gfx_path_number(path, which)` — the path's bounds in its own units:
/// left (0), top (1), width (2), height (3).
pub fn path_number(path: &Path, which: i32) -> f64 {
    let bounds = path.bounds();
    f64::from(match which {
        0 => bounds.left(),
        1 => bounds.top(),
        2 => bounds.size.width,
        3 => bounds.size.height,
        _ => panic_with(&format!("{which} is not a `std::gfx` path number")),
    })
}

/// `__gfx_path(frame, path, x, y, scale, paint, rule_or_width, join, cap,
/// miter_limit, colour)` — `Frame::path`: `path` with its origin at `(x, y)`
/// dp, `scale` dp to one path unit. `paint` 0 fills by the rule
/// `rule_or_width` (0 non-zero, 1 even-odd); 1 strokes `rule_or_width` path
/// units wide with `join` (0 miter, 1 round, 2 bevel), `cap` (0 butt, 1
/// round, 2 square) and `miter_limit`.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each: a path draw crosses \
              as numbers so nothing is allocated per draw"
)]
pub fn path(
    frame: u32,
    path: &Path,
    x: f64,
    y: f64,
    scale: f64,
    paint: i32,
    rule_or_width: f64,
    join: i32,
    cap: i32,
    miter_limit: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    let paint = match paint {
        0 => PathPaint::Fill(if rule_or_width == 0.0 {
            FillRule::NonZero
        } else {
            FillRule::EvenOdd
        }),
        1 => PathPaint::Stroke(Stroke {
            width: rule_or_width as f32,
            join: match join {
                0 => Join::Miter,
                1 => Join::Round,
                2 => Join::Bevel,
                _ => panic_with(&format!("{join} is not a `std::gfx` join code")),
            },
            cap: match cap {
                0 => Cap::Butt,
                1 => Cap::Round,
                2 => Cap::Square,
                _ => panic_with(&format!("{cap} is not a `std::gfx` cap code")),
            },
            miter_limit: miter_limit as f32,
        }),
        _ => panic_with(&format!("{paint} is not a `std::gfx` path paint code")),
    };
    let origin = Point::new(x as f32, y as f32);
    let color = color(red, green, blue, alpha);
    with_frame(frame, |builder| {
        builder.path(path, origin, scale as f32, paint, color)
    });
}

// ------------------------------------------------ the two values' faces ---

/// Identity, as every host value's is: two paths are `==` when they are one
/// (two parsed from the same data still share their masks).
impl PartialEq for Path {
    fn eq(&self, other: &Path) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }
}

impl Js for Path {
    fn js(&self) -> String {
        panic_with(
            "printing a `Path` is a host object's own inspection, which has no text form (print \
             its `bounds()`)",
        )
    }
}

impl Json for Path {
    fn json(&self) -> String {
        panic_with("a `Path` is a host object, which has no JSON form")
    }
}

impl PartialEq for PathBuilder {
    fn eq(&self, other: &PathBuilder) -> bool {
        Rc::ptr_eq(&self.builder, &other.builder)
    }
}

impl std::fmt::Debug for PathBuilder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PathBuilder")
    }
}

impl Js for PathBuilder {
    fn js(&self) -> String {
        panic_with(
            "printing a `PathBuilder` is a host object's own inspection, which has no text form \
             (finish it and print the path's `bounds()`)",
        )
    }
}

impl Json for PathBuilder {
    fn json(&self) -> String {
        panic_with("a `PathBuilder` is a host object, which has no JSON form")
    }
}
