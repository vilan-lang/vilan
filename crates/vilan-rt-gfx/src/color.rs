//! Colours and fills.

use crate::geometry::Point;

/// A colour in sRGB with straight (not premultiplied) alpha, each channel 0 to 1.
///
/// sRGB because that is how a design writes colours; the renderer blends in
/// linear light (its targets are sRGB textures, so the GPU converts on write)
/// and premultiplies in the shader.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl Color {
    pub const TRANSPARENT: Color = Color::new(0.0, 0.0, 0.0, 0.0);
    pub const BLACK: Color = Color::new(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);

    pub const fn new(red: f32, green: f32, blue: f32, alpha: f32) -> Color {
        Color {
            red,
            green,
            blue,
            alpha,
        }
    }

    /// An opaque colour from 8-bit sRGB channels, as a design tool writes them.
    pub fn rgb(red: u8, green: u8, blue: u8) -> Color {
        Color::rgba(red, green, blue, 255)
    }

    /// A colour from 8-bit sRGB channels and an 8-bit alpha.
    pub fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Color {
        Color::new(
            f32::from(red) / 255.0,
            f32::from(green) / 255.0,
            f32::from(blue) / 255.0,
            f32::from(alpha) / 255.0,
        )
    }

    /// The same colour with its alpha multiplied by `factor` (an opacity layer
    /// applies itself this way).
    pub fn with_alpha_multiplied(self, factor: f32) -> Color {
        Color {
            alpha: self.alpha * factor,
            ..self
        }
    }

    /// Channels clamped into 0..=1, a NaN read as 0, so a malformed colour
    /// cannot reach the shader as a value it would blend into garbage.
    pub(crate) fn sanitized(self) -> [f32; 4] {
        let channel = |value: f32| {
            if value.is_nan() {
                0.0
            } else {
                value.clamp(0.0, 1.0)
            }
        };
        [
            channel(self.red),
            channel(self.green),
            channel(self.blue),
            channel(self.alpha),
        ]
    }
}

/// A two-stop linear gradient between two points in the scene's coordinates
/// (dp, transformed like the rectangle it fills). Before `start` the start
/// colour holds, past `end` the end colour.
///
/// Interpolated in sRGB with premultiplied alpha, as CSS's default
/// `linear-gradient` is, so a gradient written against a design tool looks the
/// same here; interpolating in linear light would brighten every midpoint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearGradient {
    pub start: Point,
    pub end: Point,
    pub start_color: Color,
    pub end_color: Color,
}

/// How a rectangle's inside is painted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill {
    Solid(Color),
    LinearGradient(LinearGradient),
}

impl From<Color> for Fill {
    fn from(color: Color) -> Fill {
        Fill::Solid(color)
    }
}

impl From<LinearGradient> for Fill {
    fn from(gradient: LinearGradient) -> Fill {
        Fill::LinearGradient(gradient)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_malformed_colour_is_clamped_before_the_shader_sees_it() {
        assert_eq!(
            Color::new(f32::NAN, 2.0, -1.0, 0.5).sanitized(),
            [0.0, 1.0, 0.0, 0.5]
        );
    }

    #[test]
    fn eight_bit_channels_map_onto_zero_to_one() {
        assert_eq!(Color::rgb(255, 0, 51), Color::new(1.0, 0.0, 0.2, 1.0));
    }
}
