use super::tests_support::{ICONS, IconPaint, RING, STROKE_WIDTH};
use super::*;

/// An icon's paint as the crate's.
pub(crate) fn icon_paint(paint: IconPaint) -> PathPaint {
    match paint {
        IconPaint::Stroke => PathPaint::Stroke(Stroke::new(STROKE_WIDTH)),
        IconPaint::Fill => PathPaint::Fill(FillRule::NonZero),
        IconPaint::FillEvenOdd => PathPaint::Fill(FillRule::EvenOdd),
    }
}

fn segments(data: &str) -> Vec<Segment> {
    Path::from_svg(data)
        .unwrap_or_else(|error| panic!("{data:?}: {error}"))
        .data
        .segments
        .clone()
}

fn point(x: f32, y: f32) -> zeno::Point {
    zeno::Point::new(x, y)
}

fn syntax_error(data: &str) -> (usize, &'static str) {
    match Path::from_svg(data) {
        Err(PathError::Syntax { byte, reason, .. }) => (byte, reason),
        other => panic!("{data:?} parsed: {other:?}"),
    }
}

// ------------------------------------------------------- the byte, pinned ---

/// Every way path data can stop making sense names the byte where it does:
/// the wrong byte, or the data's length when it ends too soon. The first
/// five are data `zeno`'s own validator accepts or misplaces.
#[test]
fn a_parse_error_names_the_byte_where_the_data_stops_making_sense() {
    let cases: &[(&str, usize, &str)] = &[
        // Ends inside a move's coordinates: `zeno` validates this.
        ("M 10", 4, "expected a number"),
        // A command letter with no arguments at the end.
        ("M10 10 L", 8, "expected a number"),
        // An argument group cut short.
        ("M10 10 L5 5 6", 13, "expected a number"),
        ("M10 10 C1 2 3 4 5", 17, "expected a number"),
        // Does not start with a move.
        ("L10 10", 0, "path data starts with a move (M or m)"),
        ("  l1 1", 2, "path data starts with a move (M or m)"),
        // Not a command letter.
        (
            "M10 10 X 5",
            7,
            "expected a command letter (M, L, H, V, C, S, Q, T, A or Z, or m, l, h, v, c, s, q, \
             t, a or z)",
        ),
        // A number after a close.
        (
            "M0 0Z5",
            5,
            "expected a command letter (M, L, H, V, C, S, Q, T, A or Z, or m, l, h, v, c, s, q, \
             t, a or z)",
        ),
        // Two commas.
        ("M10,,10", 4, "expected a number"),
        // A comma with nothing after it.
        ("M10 10 L5,5,", 12, "expected a number after the comma"),
        ("M10 10 L5,5, L1 1", 13, "expected a number after the comma"),
        // An arc flag that is not 0 or 1.
        ("M10 10a5 5 0 2 1 3 3", 13, "expected a flag (0 or 1)"),
        ("M10 10a5 5 0 1 -1 3 3", 15, "expected a flag (0 or 1)"),
        // A sign or a dot alone is no number.
        ("M- 1", 1, "expected a number"),
        ("M1 . 1", 3, "expected a number"),
        // A number a float cannot hold.
        ("M1e39 0", 1, "a number too large to draw"),
        // A byte that is not ASCII, after a multi-byte character's start.
        (
            "M0 0 L1 1 \u{e9}",
            10,
            "expected a command letter (M, L, H, V, C, S, Q, T, A or Z, or m, l, h, v, c, s, q, \
             t, a or z)",
        ),
    ];
    for (data, byte, reason) in cases {
        assert_eq!(syntax_error(data), (*byte, *reason), "{data:?}");
    }
}

#[test]
fn the_message_shows_the_data_around_the_byte() {
    let error = Path::from_svg("M10 10 X 5").expect_err("malformed");
    assert_eq!(error.byte(), Some(7));
    assert_eq!(
        error.to_string(),
        "invalid path data at byte 7: expected a command letter (M, L, H, V, C, S, Q, T, A or Z, \
         or m, l, h, v, c, s, q, t, a or z): M10 10 >X 5"
    );
    // At the end, the marker is the last thing; a long datum is cut on both
    // sides, on character boundaries.
    let error = Path::from_svg("M10 10 L").expect_err("malformed");
    assert!(error.to_string().ends_with(": M10 10 L>"), "{error}");
    let long = format!(
        "M0 0 {} L1 \u{e9}\u{e9} q 1 1 1 1 1 1 1 1 1 1 1",
        "L1 1 ".repeat(8)
    );
    let error = Path::from_svg(&long).expect_err("malformed");
    let shown = error.to_string();
    // `L1` then a byte that starts no number: the y coordinate is missing.
    // The two-byte characters after it are kept whole.
    assert_eq!(
        shown,
        "invalid path data at byte 49: expected a number: ...1 L1 1 L1 1  L1 \
         >\u{e9}\u{e9} q 1 1 1 1 1..."
    );
}

#[test]
fn empty_data_is_an_empty_path() {
    for data in ["", "   ", "\n\t"] {
        let path = Path::from_svg(data).expect("empty data is a path");
        assert!(path.is_empty());
        assert_eq!(path.bounds(), Rect::new(0.0, 0.0, 0.0, 0.0));
    }
}

// ------------------------------------------------------- every command ---

#[test]
fn every_absolute_command_makes_its_segments() {
    assert_eq!(
        segments("M1 2 L3 4 H5 V6 C7 8 9 10 11 12 Q13 14 15 16 Z"),
        vec![
            Segment::MoveTo(point(1.0, 2.0)),
            Segment::LineTo(point(3.0, 4.0)),
            Segment::LineTo(point(5.0, 4.0)),
            Segment::LineTo(point(5.0, 6.0)),
            Segment::CurveTo(point(7.0, 8.0), point(9.0, 10.0), point(11.0, 12.0)),
            Segment::QuadTo(point(13.0, 14.0), point(15.0, 16.0)),
            Segment::Close,
        ]
    );
}

#[test]
fn every_relative_command_is_relative_to_the_current_point() {
    assert_eq!(
        segments("m1 2 l2 2 h2 v2 c2 2 4 4 6 6 q2 2 4 4 z"),
        vec![
            Segment::MoveTo(point(1.0, 2.0)),
            Segment::LineTo(point(3.0, 4.0)),
            Segment::LineTo(point(5.0, 4.0)),
            Segment::LineTo(point(5.0, 6.0)),
            Segment::CurveTo(point(7.0, 8.0), point(9.0, 10.0), point(11.0, 12.0)),
            Segment::QuadTo(point(13.0, 14.0), point(15.0, 16.0)),
            Segment::Close,
        ]
    );
}

/// `S` and `T` reflect the previous curve's last control point about the
/// current point, but only after a curve of their own kind; otherwise their
/// first control point is the current point.
#[test]
fn smooth_curves_reflect_the_previous_control_point_of_their_own_kind() {
    assert_eq!(
        segments("M0 0 C0 10 10 10 10 0 S20 -10 20 0"),
        vec![
            Segment::MoveTo(point(0.0, 0.0)),
            Segment::CurveTo(point(0.0, 10.0), point(10.0, 10.0), point(10.0, 0.0)),
            Segment::CurveTo(point(10.0, -10.0), point(20.0, -10.0), point(20.0, 0.0)),
        ]
    );
    assert_eq!(
        segments("M0 0 Q5 10 10 0 T20 0 t10 0"),
        vec![
            Segment::MoveTo(point(0.0, 0.0)),
            Segment::QuadTo(point(5.0, 10.0), point(10.0, 0.0)),
            Segment::QuadTo(point(15.0, -10.0), point(20.0, 0.0)),
            Segment::QuadTo(point(25.0, 10.0), point(30.0, 0.0)),
        ]
    );
    // After a line, and after a curve of the other kind: no reflection.
    assert_eq!(
        segments("M0 0 L10 0 S20 10 30 0")[2],
        Segment::CurveTo(point(10.0, 0.0), point(20.0, 10.0), point(30.0, 0.0))
    );
    let mixed = segments("M0 0 Q5 5 10 0 S20 10 30 0 M0 0 C1 1 2 2 3 0 T9 0");
    assert_eq!(
        mixed[2],
        Segment::CurveTo(point(10.0, 0.0), point(20.0, 10.0), point(30.0, 0.0))
    );
    assert_eq!(mixed[5], Segment::QuadTo(point(3.0, 0.0), point(9.0, 0.0)));
}

/// An arc becomes cubic curves that end EXACTLY at its end point, so the
/// relative command after it starts where the data says; SVG's
/// out-of-range rules: a zero radius is a line, an end point equal to the
/// current point is nothing, radii too small to reach are scaled up.
#[test]
fn an_arc_is_curves_to_its_exact_end_by_svgs_rules() {
    let half = segments("M0 0 A10 10 0 0 1 20 0 l1 0");
    assert_eq!(half[0], Segment::MoveTo(point(0.0, 0.0)));
    let curves: Vec<&Segment> = half[1..half.len() - 1].iter().collect();
    assert_eq!(curves.len(), 2, "a half circle is two quarter curves");
    assert!(matches!(curves[1], Segment::CurveTo(_, _, end) if *end == point(20.0, 0.0)));
    assert_eq!(
        *half.last().expect("a line"),
        Segment::LineTo(point(21.0, 0.0))
    );
    // The sweep flag picks the side: positive angles are clockwise on a
    // screen (y down), so the half circle from (0,0) to (20,0) sweeping
    // positively goes over the top, through (10,-10).
    let Segment::CurveTo(_, _, middle) = half[1] else {
        panic!("a curve")
    };
    assert!(
        (middle.x - 10.0).abs() < 1e-4 && (middle.y + 10.0).abs() < 1e-4,
        "{middle:?}"
    );
    let other_side = segments("M0 0 A10 10 0 0 0 20 0");
    let Segment::CurveTo(_, _, middle) = other_side[1] else {
        panic!("a curve")
    };
    assert!((middle.y - 10.0).abs() < 1e-4, "{middle:?}");
    // A zero radius is a straight line; an end point equal to the start is
    // nothing.
    assert_eq!(
        segments("M0 0 A0 5 0 0 1 10 10 A5 5 0 0 1 10 10"),
        vec![
            Segment::MoveTo(point(0.0, 0.0)),
            Segment::LineTo(point(10.0, 10.0)),
        ]
    );
    // Radii too small are scaled up: a 1-unit radius to a point 20 away is a
    // half circle of radius 10.
    let scaled = segments("M0 0 a1 1 0 0 1 20 0");
    let Segment::CurveTo(_, _, middle) = scaled[1] else {
        panic!("a curve")
    };
    assert!((middle.y + 10.0).abs() < 1e-3, "{middle:?}");
    // Relative arcs are relative to the current point.
    let relative = segments("M5 5 a10 10 0 0 1 20 0");
    assert!(
        matches!(relative.last(), Some(Segment::CurveTo(_, _, end)) if *end == point(25.0, 5.0))
    );
}

/// A move's later pairs are lines, relative ones relative; a close returns
/// to the contour's start, and the next drawing command starts a contour
/// there.
#[test]
fn implicit_lines_and_contours_follow_svgs_rules() {
    assert_eq!(
        segments("M0 0 1 1 2 2 m1 1 1 1"),
        vec![
            Segment::MoveTo(point(0.0, 0.0)),
            Segment::LineTo(point(1.0, 1.0)),
            Segment::LineTo(point(2.0, 2.0)),
            Segment::MoveTo(point(3.0, 3.0)),
            Segment::LineTo(point(4.0, 4.0)),
        ]
    );
    assert_eq!(
        segments("M5 5 l5 0 z l1 1 Z Z"),
        vec![
            Segment::MoveTo(point(5.0, 5.0)),
            Segment::LineTo(point(10.0, 5.0)),
            Segment::Close,
            Segment::MoveTo(point(5.0, 5.0)),
            Segment::LineTo(point(6.0, 6.0)),
            Segment::Close,
        ]
    );
}

/// Numbers and flags written as tightly as the grammar allows, exponents,
/// commas and every kind of white space.
#[test]
fn numbers_flags_and_separators_parse_as_the_grammar_allows() {
    assert_eq!(
        segments("M.5.5L10-5l+1e1,2E-1\t\n\r\x0cH-.5e+1"),
        vec![
            Segment::MoveTo(point(0.5, 0.5)),
            Segment::LineTo(point(10.0, -5.0)),
            Segment::LineTo(point(20.0, -4.8)),
            Segment::LineTo(point(-5.0, -4.8)),
        ]
    );
    // Flags need no separator: large 0, sweep 1, then the end point 10 10.
    let tight = segments("M0 0a10 10 0 0110 10");
    assert!(matches!(tight.last(), Some(Segment::CurveTo(_, _, end)) if *end == point(10.0, 10.0)));
    // `1e` is the number 1 and then a letter that is no command.
    assert_eq!(syntax_error("M1e 2").0, 2);
}

/// The seven icons of the paper's spike parse, and a relative and an
/// absolute spelling of the same shape are the same commands.
#[test]
fn the_spike_icons_parse() {
    for icon in ICONS {
        let name = icon.name;
        let path = Path::from_svg(icon.data).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(!path.is_empty(), "{name}");
        let bounds = path.bounds();
        assert!(
            bounds.left() >= 1.0 && bounds.right() <= 23.0,
            "{name}: {bounds:?} is inside the icon box"
        );
    }
}

// --------------------------------------------------- identity and bounds ---

#[test]
fn equal_data_hashes_equal_and_is_the_same_shape() {
    let first = Path::from_svg("M20 6 9 17l-5-5").expect("valid");
    let second = Path::from_svg("M 20,6 L 9,17 l -5,-5").expect("valid");
    let other = Path::from_svg("M20 6 9 17l-5-4").expect("valid");
    assert!(!Arc::ptr_eq(&first.data, &second.data));
    assert_eq!(first.data.hash, second.data.hash);
    assert!(first.same_shape(&second));
    assert_ne!(first.data.hash, other.data.hash);
    assert!(!first.same_shape(&other));
}

#[test]
fn the_bounds_hold_every_point_control_points_included() {
    let path = Path::from_svg("M2 3 C0 20 30 -5 10 10").expect("valid");
    assert_eq!(path.bounds(), Rect::new(0.0, -5.0, 30.0, 25.0));
}

// ---------------------------------------------------------------- builder ---

#[test]
fn the_builder_makes_what_the_same_svg_data_makes() {
    let mut builder = Path::builder();
    builder
        .move_to(1.0, 2.0)
        .line_to(3.0, 4.0)
        .quad_to(5.0, 6.0, 7.0, 8.0)
        .cubic_to(9.0, 10.0, 11.0, 12.0, 13.0, 14.0)
        .arc_to(5.0, 5.0, 0.0, false, true, 23.0, 14.0)
        .close()
        .line_to(0.0, 0.0);
    let built = builder.finish().expect("finite");
    let parsed = Path::from_svg("M1 2 L3 4 Q5 6 7 8 C9 10 11 12 13 14 A5 5 0 0 1 23 14 Z L0 0")
        .expect("valid");
    assert!(built.same_shape(&parsed));
    // The builder keeps its segments: another one extends it.
    builder.line_to(9.0, 9.0);
    assert_eq!(
        builder.finish().expect("finite").data.segments.len(),
        parsed.data.segments.len() + 1
    );
    // A segment with no contour open starts one at the origin.
    let mut builder = Path::builder();
    builder.line_to(4.0, 0.0);
    assert_eq!(
        builder.finish().expect("finite").data.segments,
        vec![
            Segment::MoveTo(point(0.0, 0.0)),
            Segment::LineTo(point(4.0, 0.0))
        ]
    );
}

#[test]
fn a_coordinate_that_is_not_finite_is_reported_by_finish() {
    let mut builder = Path::builder();
    builder
        .move_to(0.0, 0.0)
        .line_to(1.0, 1.0)
        .arc_to(f32::NAN, 1.0, 0.0, false, false, 2.0, 2.0)
        .line_to(f32::INFINITY, 0.0);
    let error = builder.finish().expect_err("not finite");
    assert_eq!(error, PathError::NotFinite { segment: 2 });
    assert_eq!(
        error.to_string(),
        "a path's segment 2 has a coordinate, radius or rotation that is not a finite number"
    );
}

// ------------------------------------------------- masks: rules, outlines ---

fn mask(path: &Path, paint: PathPaint, scale: f32) -> RasterizedMask {
    match rasterize(
        &PathMaskKey::new(path, paint, scale),
        &mut zeno::Scratch::new(),
        4096,
    ) {
        Rasterized::Mask(mask) => mask,
        other => panic!("no mask: {other:?}"),
    }
}

fn coverage(mask: &RasterizedMask, x: i32, y: i32) -> u8 {
    let column = (x - mask.left) as usize;
    let row = (y - mask.top) as usize;
    mask.pixels[row * mask.width as usize + column]
}

/// The ring's two circles wind the same way: non-zero fills the hole,
/// even-odd leaves it empty; the ring itself is inked either way.
#[test]
fn both_fill_rules_decide_the_rings_hole() {
    let ring = Path::from_svg(RING).expect("valid");
    let non_zero = mask(&ring, PathPaint::Fill(FillRule::NonZero), 1.0);
    let even_odd = mask(&ring, PathPaint::Fill(FillRule::EvenOdd), 1.0);
    assert_eq!((non_zero.left, non_zero.top), (2, 2));
    assert_eq!((non_zero.width, non_zero.height), (20, 20));
    // The centre pixel, (12, 12) to (13, 13), inside the hole.
    assert_eq!(coverage(&non_zero, 12, 12), 255);
    assert_eq!(coverage(&even_odd, 12, 12), 0);
    // A pixel of the ring, between the radii 5 and 10 above the centre.
    assert_eq!(coverage(&non_zero, 11, 4), 255);
    assert_eq!(coverage(&even_odd, 11, 4), 255);
}

/// A stroke's outline is made once per style and reused at every size; a
/// second style keeps a second; and filling the kept outline draws the
/// bytes stroking directly draws.
#[test]
fn a_strokes_outline_is_kept_per_style_and_reused() {
    for icon in ICONS {
        let (name, paint) = (icon.name, icon_paint(icon.paint));
        let PathPaint::Stroke(stroke) = &paint else {
            continue;
        };
        let path = Path::from_svg(icon.data).expect("valid");
        assert_eq!(path.kept_outlines(), 0);
        let first = path.outline(stroke);
        let again = path.outline(stroke);
        assert!(Arc::ptr_eq(&first, &again), "{name}: the outline is reused");
        for size in [16.0, 24.0, 48.0, 256.0] {
            let scale = size / 24.0;
            let kept = mask(&path, paint, scale);
            assert_eq!(path.kept_outlines(), 1, "{name}: one style, one outline");
            let (direct, placement) = zeno::Mask::new(&path.data.segments[..])
                .style(stroke.zeno())
                .transform(Some(zeno::Transform::scale(scale, scale)))
                .render();
            assert_eq!(
                (kept.left, kept.top, kept.width, kept.height),
                (
                    placement.left,
                    placement.top,
                    placement.width,
                    placement.height
                ),
                "{name} at {size}"
            );
            assert!(kept.pixels == direct, "{name} at {size}: the same bytes");
        }
        let thinner = stroke.with_cap(Cap::Butt);
        mask(&path, PathPaint::Stroke(thinner), 1.0);
        assert_eq!(path.kept_outlines(), 2, "{name}: a second style");
        assert!(Arc::ptr_eq(&first, &path.outline(stroke)));
    }
}

#[test]
fn a_path_keeps_a_bounded_number_of_outlines() {
    let path = Path::from_svg("M0 0 L10 10").expect("valid");
    for step in 0..10 {
        path.outline(&Stroke::new(1.0 + step as f32));
    }
    assert_eq!(path.kept_outlines(), KEPT_OUTLINES);
}

/// Strokes that differ only in a miter limit their join never reads share
/// an outline and a mask.
#[test]
fn a_miter_limit_only_counts_for_miter_joins() {
    let round = Stroke::new(2.0);
    assert_eq!(round.key(), round.with_miter_limit(9.0).key());
    let miter = round.with_join(Join::Miter);
    assert_ne!(miter.key(), miter.with_miter_limit(9.0).key());
}

/// `zeno`'s stroker bevels every miter join sharper than 90 degrees,
/// whatever the limit; SVG mitres a 53-degree corner under the default
/// limit of 4 (its miter is 2.24 widths long). Lucide's strokes are round,
/// so no icon of the paper's sweep meets it.
#[test]
#[ignore = "F175: zeno 0.3.3 bevels miter joins sharper than 90 degrees inside the miter limit"]
fn a_sharp_miter_join_reaches_its_miter() {
    let path = Path::from_svg("M0 0 L10 5 L0 10").expect("valid");
    let stroke = Stroke::new(1.0).with_join(Join::Miter);
    let outline = path.outline(&stroke);
    let tip = outline
        .iter()
        .filter_map(|segment| match segment {
            Segment::MoveTo(to) | Segment::LineTo(to) => Some(to.x),
            _ => None,
        })
        .fold(f32::MIN, f32::max);
    assert!((tip - 11.118).abs() < 0.01, "the miter's tip is at {tip}");
}

#[test]
fn a_zero_width_stroke_and_an_empty_path_have_no_mask() {
    let path = Path::from_svg("M0 0 L10 10").expect("valid");
    let key = PathMaskKey::new(&path, PathPaint::Stroke(Stroke::new(0.0)), 1.0);
    assert!(matches!(
        rasterize(&key, &mut zeno::Scratch::new(), 4096),
        Rasterized::Empty
    ));
    let empty = Path::from_svg("").expect("valid");
    let key = PathMaskKey::new(&empty, PathPaint::Fill(FillRule::NonZero), 1.0);
    assert!(matches!(
        rasterize(&key, &mut zeno::Scratch::new(), 4096),
        Rasterized::Empty
    ));
    // A fill of no area: a line.
    let key = PathMaskKey::new(&path, PathPaint::Fill(FillRule::NonZero), 1.0);
    assert!(matches!(
        rasterize(&key, &mut zeno::Scratch::new(), 4096),
        Rasterized::Empty
    ));
}

#[test]
fn a_mask_larger_than_the_largest_texture_is_refused_before_it_is_made() {
    let path = Path::from_svg("M0 0 H5000 V10 H0 Z").expect("valid");
    let key = PathMaskKey::new(&path, PathPaint::Fill(FillRule::NonZero), 1.0);
    assert!(matches!(
        rasterize(&key, &mut zeno::Scratch::new(), 4096),
        Rasterized::TooLarge
    ));
}
