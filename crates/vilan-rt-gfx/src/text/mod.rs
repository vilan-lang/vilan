//! Text: shaping and paragraph layout in dp, glyph rasterisation for the atlas
//! (native-ui.md 3.4; rulings R3 and R15).
//!
//! # One Rust stack, behind a small interface
//!
//! [`TextSystem`] is the public face. Underneath it is [`TextEngine`], a
//! crate-private trait with three jobs: register font data, shape a string in
//! a style into positioned glyphs, and rasterise one glyph at one device size.
//! `parley` (shaping, bidi, line breaking, font fallback) and `swash`
//! (rasterising outlines, colour outlines and colour bitmaps) implement it
//! today ([`parley_engine`]). R15 keeps one stack first and the interface small
//! enough that a platform engine (CoreText, DirectWrite) can implement the
//! same three calls later; nothing outside this module names `parley`.
//!
//! # Shaped in dp, rasterised in device pixels
//!
//! A [`TextLayout`] is shaped ONCE, in dp, with fractional positions (no
//! quantisation, no hinting). Unhinted outlines scale linearly, so the same
//! layout serves every scale factor, text scale and transform scale: the
//! renderer multiplies each glyph's position into device pixels at encode
//! time, snaps the baseline to the pixel grid, splits the pen's x into a whole
//! pixel and one of [`SUBPIXEL_BINS`] quarter-pixel bins, and asks for that
//! glyph at that device size. A window moved to a 150% display redraws without
//! reshaping a single paragraph; the framework above keeps its layouts across
//! frames and reshapes only text whose string or style changed.
//!
//! No LCD subpixel text (rulings 3.4): it needs dual-source blending, which
//! WebGL2 lacks, and it is wrong on rotated or transparent surfaces. Glyphs are
//! greyscale coverage (or colour, for emoji).

pub(crate) mod editing;
pub(crate) mod parley_engine;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::GfxError;
use crate::geometry::{Point, Rect, Size};
use editing::EditingGeometry;

/// Horizontal subpixel positions a glyph is rasterised at: a quarter pixel.
/// Four is GPUI's choice and the usual one: finer bins multiply atlas entries
/// for a difference no reader sees at UI sizes. Vertical positions are not
/// binned, because the baseline is snapped to whole device pixels.
pub const SUBPIXEL_BINS: u8 = 4;

/// A generic family or a family by name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FontFamily {
    SansSerif,
    Serif,
    Monospace,
    /// A family by its name: an installed font, or one registered with
    /// [`TextSystem::register_font`].
    Named(String),
}

/// How a run of text looks. Every length is in dp.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub family: FontFamily,
    /// The font size in dp.
    pub size: f32,
    /// The distance between baselines as a multiple of `size`.
    pub line_height: f32,
    /// 100 (thin) to 900 (black); 400 is regular, 700 bold.
    pub weight: f32,
    pub italic: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            family: FontFamily::SansSerif,
            size: 14.0,
            line_height: 1.25,
            weight: 400.0,
            italic: false,
        }
    }
}

/// A shaped paragraph: positioned glyphs in dp, ready to be drawn any number
/// of times, at any scale, by [`SceneBuilder::text`](crate::SceneBuilder::text).
///
/// Cheap to clone (the glyphs are shared), immutable, and independent of the
/// display it is drawn on. It belongs to the [`TextSystem`] that shaped it:
/// the renderer rasterises its glyphs through that system.
#[derive(Clone, Debug)]
pub struct TextLayout {
    pub(crate) data: Arc<LayoutData>,
}

#[derive(Debug)]
pub(crate) struct LayoutData {
    pub(crate) system: u32,
    pub(crate) size: Size,
    pub(crate) line_count: usize,
    pub(crate) first_baseline: f32,
    pub(crate) runs: Vec<ShapedRun>,
    /// Where each grapheme is, for a layout shaped for editing
    /// ([`TextSystem::editable_layout`]); `None` for every other.
    pub(crate) editing: Option<EditingGeometry>,
}

/// A run of glyphs in one font instance at one size.
#[derive(Debug)]
pub(crate) struct ShapedRun {
    /// The engine's handle for the font, its variation coordinates and its
    /// synthesis: everything a rasteriser needs beyond the glyph and the size.
    pub(crate) font: u32,
    /// The font size in dp.
    pub(crate) font_size: f32,
    pub(crate) glyphs: Vec<ShapedGlyph>,
}

/// A glyph's pen position in dp, relative to the layout's top-left corner:
/// `y` is the glyph's baseline.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShapedGlyph {
    pub(crate) id: u32,
    pub(crate) x: f32,
    pub(crate) y: f32,
}

impl TextLayout {
    /// The paragraph's size in dp: the widest line, and the sum of the line
    /// heights.
    pub fn size(&self) -> Size {
        self.data.size
    }

    pub fn line_count(&self) -> usize {
        self.data.line_count
    }

    /// The first line's baseline, in dp from the top (for aligning text with
    /// other content on its baseline).
    pub fn first_baseline(&self) -> f32 {
        self.data.first_baseline
    }

    /// Glyphs in the layout (one per positioned glyph, including spaces).
    pub fn glyph_count(&self) -> usize {
        self.data.runs.iter().map(|run| run.glyphs.len()).sum()
    }

    /// Whether it was shaped for editing, so the caret questions below have
    /// answers ([`TextSystem::editable_layout`]).
    pub fn is_editable(&self) -> bool {
        self.data.editing.is_some()
    }

    /// The caret at byte `index` of the text (snapped to the start of the
    /// grapheme it is inside): a `width` wide rectangle at the grapheme's
    /// leading edge, as tall as its line, in dp from the layout's top-left.
    /// `None` for a layout not shaped for editing. See `editing.rs`.
    pub fn caret(&self, index: usize, width: f32) -> Option<Rect> {
        Some(self.data.editing.as_ref()?.caret(index, width))
    }

    /// The caret position (a byte index on a grapheme boundary) nearest
    /// `point`, in dp from the layout's top-left.
    pub fn index_at(&self, point: Point) -> Option<usize> {
        Some(self.data.editing.as_ref()?.index_at(point))
    }

    /// The next (`forward`) or previous grapheme boundary from byte `index`,
    /// or with `word` the end of the next word or the start of the previous
    /// one.
    pub fn step(&self, index: usize, forward: bool, word: bool) -> Option<usize> {
        Some(self.data.editing.as_ref()?.step(index, forward, word))
    }

    /// The start of the grapheme under `point` (byte index), in dp from the
    /// layout's top-left.
    pub fn grapheme_at(&self, point: Point) -> Option<usize> {
        Some(self.data.editing.as_ref()?.grapheme_at(point))
    }

    /// The run of word (or of other) graphemes around the grapheme starting
    /// at byte `index`, as byte indices `start..end`: a double click's word.
    pub fn word_around(&self, index: usize) -> Option<(usize, usize)> {
        Some(self.data.editing.as_ref()?.word_around(index))
    }

    /// The rectangles the graphemes between byte indices `from` and `to`
    /// cover, merged where they touch on a line.
    pub fn selection(&self, from: usize, to: usize) -> Option<Vec<Rect>> {
        Some(self.data.editing.as_ref()?.selection(from, to))
    }
}

/// One glyph at one device size and subpixel position: the atlas's key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct GlyphRasterKey {
    /// Which [`TextSystem`] the font handle belongs to.
    pub(crate) system: u32,
    pub(crate) font: u32,
    pub(crate) glyph: u32,
    /// The device pixel size, as `f32` bits (sizes are compared exactly: the
    /// same layout at the same scale produces the same bits).
    pub(crate) size_bits: u32,
    /// 0 to [`SUBPIXEL_BINS`] - 1: the pen's offset in quarter pixels.
    pub(crate) subpixel_bin: u8,
}

/// A rasterised glyph: where its bitmap sits relative to the pen position
/// (`left` right of it, `top` above the baseline), and its pixels.
#[derive(Debug)]
pub(crate) struct RasterizedGlyph {
    pub(crate) left: i32,
    pub(crate) top: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) content: GlyphContent,
}

#[derive(Debug)]
pub(crate) enum GlyphContent {
    /// One coverage byte per pixel: a monochrome glyph, tinted by the run's
    /// colour.
    Mask(Vec<u8>),
    /// Four bytes per pixel, sRGB with straight alpha: a colour glyph (emoji),
    /// drawn as it is.
    Color(Vec<u8>),
}

/// The interface a text stack implements (R15). See the module documentation.
pub(crate) trait TextEngine: Send {
    /// Registers every face in `data` (a TrueType/OpenType file or
    /// collection), answering the family names it added.
    fn register_font(&mut self, data: Vec<u8>) -> Vec<String>;

    /// Shapes `text` in `style`, wrapping at `max_width` dp when given, and
    /// with `editing` records where each grapheme is (`editing.rs`). The
    /// answer's `system` field is filled by the caller.
    fn shape(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
        editing: bool,
    ) -> LayoutData;

    /// Rasterises one glyph. `None` when the font cannot produce it (an id
    /// past the font's glyph count, or a font that no longer loads).
    fn rasterize(
        &mut self,
        font: u32,
        glyph: u32,
        size: f32,
        subpixel_offset: f32,
    ) -> Option<RasterizedGlyph>;
}

/// The fonts a program draws with, and the engine that shapes with them.
///
/// One per program is the intended use (font discovery is not free); a
/// renderer draws layouts from the system it is handed with each frame.
pub struct TextSystem {
    id: u32,
    engine: Box<dyn TextEngine>,
}

/// Every system gets its own id, so glyph keys from two systems can never
/// collide in one renderer's atlas.
fn next_system_id() -> u32 {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl Default for TextSystem {
    fn default() -> Self {
        TextSystem::new()
    }
}

impl TextSystem {
    /// A text system over the operating system's installed fonts.
    pub fn new() -> TextSystem {
        TextSystem::from_engine(Box::new(parley_engine::ParleyEngine::new(true)))
    }

    /// A text system that sees ONLY the fonts registered with
    /// [`register_font`](TextSystem::register_font): every generic family
    /// (sans-serif, serif, monospace, system-ui) resolves to them, and a
    /// character none of them has draws nothing. For output that must not depend on
    /// the machine (golden images), and for an app that ships its own fonts.
    pub fn without_system_fonts() -> TextSystem {
        TextSystem::from_engine(Box::new(parley_engine::ParleyEngine::new(false)))
    }

    pub(crate) fn from_engine(engine: Box<dyn TextEngine>) -> TextSystem {
        TextSystem {
            id: next_system_id(),
            engine,
        }
    }

    /// Registers the faces in a font file's bytes, answering the family names
    /// they were registered under. Refuses data holding no face it can read.
    pub fn register_font(&mut self, data: Vec<u8>) -> Result<Vec<String>, GfxError> {
        let families = self.engine.register_font(data);
        if families.is_empty() {
            return Err(GfxError::UnreadableFont);
        }
        Ok(families)
    }

    /// Shapes `text` in `style` on one line, or wrapped at `max_width` dp.
    pub fn layout(&mut self, text: &str, style: &TextStyle, max_width: Option<f32>) -> TextLayout {
        self.shape(text, style, max_width, false)
    }

    /// [`layout`](TextSystem::layout), recording where each grapheme is: the
    /// layout answers [`TextLayout::caret`] and the other editing questions.
    pub fn editable_layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> TextLayout {
        self.shape(text, style, max_width, true)
    }

    fn shape(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
        editing: bool,
    ) -> TextLayout {
        let mut data = self.engine.shape(text, style, max_width, editing);
        data.system = self.id;
        TextLayout {
            data: Arc::new(data),
        }
    }

    pub(crate) fn rasterize(&mut self, key: &GlyphRasterKey) -> Option<RasterizedGlyph> {
        if key.system != self.id {
            return None;
        }
        let subpixel_offset = f32::from(key.subpixel_bin) / f32::from(SUBPIXEL_BINS);
        self.engine.rasterize(
            key.font,
            key.glyph,
            f32::from_bits(key.size_bits),
            subpixel_offset,
        )
    }
}

/// Splits a device-pixel pen position into the whole pixel the glyph's bitmap
/// is placed from and its subpixel bin. A position that rounds up into the
/// next whole pixel's bin 0 moves to that pixel, so bins are always
/// `0..SUBPIXEL_BINS`.
pub(crate) fn subpixel_split(device_x: f32) -> (f32, u8) {
    let whole = device_x.floor();
    let bins = f32::from(SUBPIXEL_BINS);
    let bin = ((device_x - whole) * bins).round();
    if bin >= bins {
        (whole + 1.0, 0)
    } else {
        // `bin` is a whole number in 0..SUBPIXEL_BINS here.
        (whole, bin as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pen_position_splits_into_a_pixel_and_a_quarter_bin() {
        assert_eq!(subpixel_split(10.0), (10.0, 0));
        assert_eq!(subpixel_split(10.25), (10.0, 1));
        assert_eq!(subpixel_split(10.5), (10.0, 2));
        assert_eq!(subpixel_split(10.7), (10.0, 3));
        // 10.9 is nearer 11.0 than 10.75: the next pixel's bin 0.
        assert_eq!(subpixel_split(10.9), (11.0, 0));
        assert_eq!(subpixel_split(-0.1), (0.0, 0));
    }
}
