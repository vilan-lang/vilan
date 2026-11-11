//! The display pixel, and how it becomes device pixels (native-ui.md section 5;
//! rulings R5's section and R16).
//!
//! A scene is written in dp. Rendering it takes two numbers from the system:
//!
//! - the display's **scale factor** (1.0, 1.25, 1.5, 2.0 ...): device pixels
//!   per logical pixel;
//! - the user's **text scale** (1.0 unless they asked for larger text).
//!
//! Spacing, sizing and text are multiplied by both (the **content scale**), so
//! a larger text setting zooms the whole layout and nothing overflows its box.
//! Border widths and hairlines are multiplied by the scale factor ONLY: a
//! border is a line, not a size, and does not get heavier because the reader
//! asked for larger letters.
//!
//! Then everything lands on the device pixel grid:
//!
//! - A rectangle's edges are rounded to the nearest device pixel, each edge on
//!   its own, so rectangles that share an edge in dp share it on the device too
//!   (no seam, no overlap).
//! - A span (a rectangle's width or height) under 2 dp, and every border or
//!   hairline width under 2 dp, is rounded DOWN instead, and never below one
//!   device pixel (R16): at 150% a `dp(1)` border is one device pixel, not two,
//!   so a hairline stays a hairline at every scale, and it is never so thin it
//!   disappears. Rounding a short span's edges independently would make the
//!   same 1 dp divider one pixel tall at one position and two at the next.
//! - Widths of 2 dp and more round to the nearest device pixel.

use crate::GfxError;

/// The two scales a frame is drawn at. See the module documentation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Units {
    scale_factor: f32,
    text_scale: f32,
}

impl Default for Units {
    fn default() -> Self {
        Units {
            scale_factor: 1.0,
            text_scale: 1.0,
        }
    }
}

/// Spans and widths below this many dp follow the hairline rule (R16).
pub(crate) const HAIRLINE_LIMIT_DP: f32 = 2.0;

impl Units {
    /// Refuses a scale that is not a finite, positive number: a zero or
    /// negative scale would collapse or mirror the whole frame.
    pub fn new(scale_factor: f32, text_scale: f32) -> Result<Units, GfxError> {
        let valid = |value: f32| value.is_finite() && value > 0.0;
        if !valid(scale_factor) || !valid(text_scale) {
            return Err(GfxError::InvalidUnits {
                scale_factor,
                text_scale,
            });
        }
        Ok(Units {
            scale_factor,
            text_scale,
        })
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub fn text_scale(&self) -> f32 {
        self.text_scale
    }

    /// Device pixels per dp for spacing, sizing and text.
    pub fn content_scale(&self) -> f32 {
        self.scale_factor * self.text_scale
    }

    /// A span of `length` dp starting at `start` dp, in device pixels: the
    /// snapped start and the snapped length. See the module documentation.
    pub(crate) fn snap_span(&self, start: f32, length: f32) -> (f32, f32) {
        let scale = self.content_scale();
        let device_start = (start * scale).round();
        if length.is_nan() || length <= 0.0 {
            return (device_start, 0.0);
        }
        if length < HAIRLINE_LIMIT_DP {
            return (device_start, (length * scale).floor().max(1.0));
        }
        let device_end = ((start + length) * scale).round();
        (device_start, (device_end - device_start).max(1.0))
    }

    /// A border or hairline of `width` dp, in whole device pixels: the scale
    /// factor only (not the text scale), the hairline rule under 2 dp, and
    /// zero only for a width that is zero (or negative, or NaN).
    pub fn line_width(&self, width: f32) -> f32 {
        if width.is_nan() || width <= 0.0 {
            return 0.0;
        }
        let raw = width * self.scale_factor;
        if width < HAIRLINE_LIMIT_DP {
            raw.floor().max(1.0)
        } else {
            raw.round().max(1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(scale_factor: f32) -> Units {
        Units::new(scale_factor, 1.0).expect("valid units")
    }

    #[test]
    fn a_one_dp_border_is_one_device_pixel_up_to_two_hundred_percent() {
        assert_eq!(units(1.0).line_width(1.0), 1.0);
        assert_eq!(units(1.25).line_width(1.0), 1.0);
        // R16: 1.5 device pixels rounds DOWN to one, not up to two.
        assert_eq!(units(1.5).line_width(1.0), 1.0);
        assert_eq!(units(2.0).line_width(1.0), 2.0);
    }

    #[test]
    fn a_border_is_never_thinner_than_one_device_pixel() {
        assert_eq!(units(1.0).line_width(0.25), 1.0);
        assert_eq!(units(1.25).line_width(0.5), 1.0);
        assert_eq!(units(1.0).line_width(0.0), 0.0);
        assert_eq!(units(1.0).line_width(-1.0), 0.0);
        assert_eq!(units(1.0).line_width(f32::NAN), 0.0);
    }

    #[test]
    fn a_border_of_two_dp_or_more_rounds_to_the_nearest_pixel() {
        assert_eq!(units(1.25).line_width(2.0), 3.0); // 2.5 -> 3
        assert_eq!(units(1.5).line_width(3.0), 5.0); // 4.5 -> 5
        assert_eq!(units(1.25).line_width(3.0), 4.0); // 3.75 -> 4
        // 1.5 dp is still a hairline: 2.25 rounds down to 2.
        assert_eq!(units(1.5).line_width(1.5), 2.0);
    }

    #[test]
    fn the_text_scale_does_not_thicken_a_border() {
        let large_text = Units::new(1.5, 2.0).expect("valid units");
        assert_eq!(large_text.line_width(1.0), 1.0);
        assert_eq!(large_text.content_scale(), 3.0);
    }

    #[test]
    fn a_short_span_is_the_same_height_wherever_it_sits() {
        let scale = units(1.25);
        for step in 0..16 {
            let top = step as f32 * 0.8;
            let (_, height) = scale.snap_span(top, 1.0);
            assert_eq!(height, 1.0, "a 1 dp divider at y={top}");
        }
    }

    #[test]
    fn spans_that_share_an_edge_in_dp_share_it_on_the_device() {
        for scale_factor in [1.0, 1.25, 1.5, 2.0] {
            let scale = units(scale_factor);
            let mut position = 0.0;
            for width in [10.3, 7.7, 21.1, 3.3] {
                let (start, length) = scale.snap_span(position, width);
                let (next_start, _) = scale.snap_span(position + width, 5.0);
                assert_eq!(start + length, next_start, "at {scale_factor}");
                position += width;
            }
        }
    }

    #[test]
    fn a_scale_that_is_not_a_positive_number_is_refused() {
        assert!(Units::new(0.0, 1.0).is_err());
        assert!(Units::new(1.0, -1.0).is_err());
        assert!(Units::new(f32::INFINITY, 1.0).is_err());
        assert!(Units::new(f32::NAN, 1.0).is_err());
    }
}
