//! `std::ui`'s icons on the program's side (gfx-path.md S3): the
//! `__ui_set_icon` extern, which parses an `Icon`'s path data and hands the
//! tree its shape (`ui/icon.rs`, where the rules are).
//!
//! # Parsing, once per data
//!
//! An `Icon` crosses as its SVG path data, its box and its paint's numbers,
//! and is parsed HERE, the first time its data is set on an element: parsed
//! paths are kept by their data, so every `check()` of a program (a row's
//! mark in each of a thousand rows) is one parse, and one mask, since the
//! renderer keeps masks by content. The keep is bounded: past
//! [`PARSED_LIMIT`] different data it starts again (an element holds its own
//! parsed path, so nothing drawn is lost; a program that makes path data at
//! run time parses it again at worst).
//!
//! # Malformed data fails the program
//!
//! An icon's data is written in the source (or generated into it), so data
//! that does not parse is a mistake in the program, not a condition to
//! handle: it fails the program where the icon is given to an element, with
//! `Path::from_svg`'s reason, which names the byte and shows the data around
//! it. Drawing nothing would hide it. A program that takes path data from
//! outside checks it first with `std::gfx`'s `Path::from_svg`, which answers
//! a `Result`.

use std::cell::RefCell;
use std::collections::HashMap;

use vilan_rt::{Str, panic_with};

use super::{Node, change, or_fail};
use crate::path::Path;
use crate::ui::IconShape;

/// How many different path data the program's keep holds before it starts
/// again: far more than an app's icons, few enough that a program making
/// data at run time cannot grow it without end.
const PARSED_LIMIT: usize = 512;

thread_local! {
    /// Every path data parsed for an icon, by its text.
    static PARSED: RefCell<HashMap<String, Path>> = RefCell::new(HashMap::new());
}

/// `data`, parsed: kept from an earlier icon of the same data, or parsed
/// now. Data that does not parse fails the program, naming the byte.
fn parsed(data: &str) -> Path {
    if let Some(path) = PARSED.with(|parsed| parsed.borrow().get(data).cloned()) {
        return path;
    }
    let path = match Path::from_svg(data) {
        Ok(path) => path,
        Err(error) => panic_with(&format!("`std::ui`: an icon's path data: {error}")),
    };
    PARSED.with(|parsed| {
        let mut parsed = parsed.borrow_mut();
        if parsed.len() >= PARSED_LIMIT {
            parsed.clear();
        }
        parsed.insert(data.to_string(), path.clone());
    });
    path
}

/// `__ui_set_icon(node, data, view_box, paint, rule_or_width, join, cap,
/// miter_limit)` — an icon's shape (`of`): SVG path data in a square box
/// `view_box` path units on a side, painted as `__gfx_path` paints (`paint` 0
/// fills by the rule `rule_or_width`, 1 strokes `rule_or_width` units wide
/// with `join`, `cap` and `miter_limit`).
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each: an icon's paint \
              crosses as `__gfx_path`'s numbers"
)]
pub fn set_icon(
    node: &Node,
    data: &Str,
    view_box: f64,
    paint: i32,
    rule_or_width: f64,
    join: i32,
    cap: i32,
    miter_limit: f64,
) {
    let paint = super::super::paths::path_paint(paint, rule_or_width, join, cap, miter_limit);
    let shape = or_fail(IconShape::new(parsed(data), view_box as f32, paint));
    change(node, |tree, id| or_fail(tree.set_icon(id, shape)));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vilan_rt::str_new;

    use super::super::{node, with_tree};
    use super::*;

    const CHECK: &str = "M20 6 9 17l-5-5";

    /// gfx-path.md 4.1: two icons of the same data are one parse (and so
    /// one path, whose masks the renderer keeps by content).
    #[test]
    fn two_icons_of_the_same_data_share_one_parse() {
        let first = node(7);
        let second = node(7);
        let other = node(7);
        set_icon(&first, &str_new(CHECK), 24.0, 1, 2.0, 1, 1, 4.0);
        set_icon(&second, &str_new(CHECK), 24.0, 1, 2.0, 1, 1, 4.0);
        set_icon(&other, &str_new("m6 9 6 6 6-6"), 24.0, 1, 2.0, 1, 1, 4.0);
        with_tree(|tree| {
            let path = |of: &Node| tree.icon_shape(of.id()).expect("a shape").path().clone();
            assert!(Arc::ptr_eq(&path(&first).data, &path(&second).data));
            assert!(!Arc::ptr_eq(&path(&first).data, &path(&other).data));
        });
    }

    /// Malformed data fails the program where the icon is set, with the
    /// parser's reason naming the byte: never an icon that draws nothing.
    #[test]
    fn malformed_data_fails_the_program_naming_the_byte() {
        let icon = node(7);
        let failure = std::panic::catch_unwind(|| {
            set_icon(&icon, &str_new("M10 10 L"), 24.0, 1, 2.0, 1, 1, 4.0)
        })
        .expect_err("malformed data fails");
        let message = failure.downcast_ref::<String>().expect("a message");
        assert!(
            message.contains("an icon's path data: invalid path data at byte 8"),
            "{message}"
        );
        assert!(with_tree(|tree| tree.icon_shape(icon.id()).is_none()));
    }

    /// A box that is not a positive number fails as plainly.
    #[test]
    fn a_box_that_is_not_positive_fails_the_program() {
        let icon = node(7);
        let failure =
            std::panic::catch_unwind(|| set_icon(&icon, &str_new(CHECK), 0.0, 1, 2.0, 1, 1, 4.0))
                .expect_err("a zero box fails");
        let message = failure.downcast_ref::<String>().expect("a message");
        assert!(
            message.contains("an icon's box must be a positive number"),
            "{message}"
        );
    }
}
