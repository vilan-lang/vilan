//! The text engine on `parley` (shaping, layout) and `swash` (rasterising).

use std::collections::HashMap;

use fontique::{Blob, Collection, CollectionOptions, GenericFamily, SourceCache};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontData, FontStyle, FontWeight, Layout,
    LayoutContext, LineHeight, OverflowWrap, PositionedLayoutItem, StyleProperty,
};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Angle, Format, Transform, Vector};

use super::editing::{EditingGeometry, GraphemeBox, LineBox};
use super::{
    FontFamily, GlyphContent, LayoutData, RasterizedGlyph, ShapedGlyph, ShapedRun, Shaping,
    TextEngine, TextStyle,
};
use crate::geometry::Size;

/// Every generic family a registered font answers for when the engine sees no
/// system fonts (`TextSystem::without_system_fonts`).
const GENERIC_FAMILIES: [GenericFamily; 5] = [
    GenericFamily::SansSerif,
    GenericFamily::Serif,
    GenericFamily::Monospace,
    GenericFamily::SystemUi,
    GenericFamily::UiSansSerif,
];

/// A font as a rasteriser needs it: the face, its variation coordinates and
/// the synthesis parley chose (a bold or italic the family does not have).
struct FontInstance {
    data: FontData,
    coordinates: Vec<i16>,
    embolden: bool,
    skew_degrees: Option<f32>,
}

/// The interning key of a [`FontInstance`]: the face's blob and index, the
/// coordinates' bits and the synthesis' bits.
#[derive(Clone, PartialEq, Eq, Hash)]
struct FontInstanceKey {
    blob: u64,
    index: u32,
    coordinates: Vec<i16>,
    embolden: bool,
    skew_bits: Option<u32>,
}

pub(crate) struct ParleyEngine {
    font_context: FontContext,
    layout_context: LayoutContext<()>,
    scale_context: ScaleContext,
    sees_system_fonts: bool,
    instances: Vec<FontInstance>,
    instance_ids: HashMap<FontInstanceKey, u32>,
}

impl ParleyEngine {
    pub(crate) fn new(system_fonts: bool) -> ParleyEngine {
        let collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts,
        });
        ParleyEngine {
            font_context: FontContext {
                collection,
                source_cache: SourceCache::default(),
            },
            layout_context: LayoutContext::new(),
            scale_context: ScaleContext::new(),
            sees_system_fonts: system_fonts,
            instances: Vec::new(),
            instance_ids: HashMap::new(),
        }
    }

    fn intern(&mut self, run: &parley::Run<'_, ()>) -> u32 {
        let font = run.font();
        let synthesis = run.synthesis();
        let coordinates: Vec<i16> = run
            .normalized_coords()
            .iter()
            .map(|coordinate| coordinate.to_bits())
            .collect();
        let key = FontInstanceKey {
            blob: font.data.id(),
            index: font.index,
            coordinates: coordinates.clone(),
            embolden: synthesis.embolden(),
            skew_bits: synthesis.skew().map(f32::to_bits),
        };
        if let Some(id) = self.instance_ids.get(&key) {
            return *id;
        }
        let id = u32::try_from(self.instances.len()).expect("fewer than 2^32 font instances");
        self.instances.push(FontInstance {
            data: font.clone(),
            coordinates,
            embolden: synthesis.embolden(),
            skew_degrees: synthesis.skew(),
        });
        self.instance_ids.insert(key, id);
        id
    }
}

/// Where each grapheme of `layout` is (`editing.rs`): parley's clusters are
/// graphemes (intersected with its runs), and each line's glyph runs carry
/// their clusters in visual order from the run's offset. What each grapheme
/// is, and the line breaks of lines parley draws no cluster for, are
/// `EditingGeometry::new`'s.
fn editing_geometry(layout: &Layout<()>, text: &str, style: &TextStyle) -> EditingGeometry {
    let mut graphemes = Vec::new();
    let mut lines = Vec::new();
    for (index, line) in layout.lines().enumerate() {
        let metrics = line.metrics();
        let bytes = line.text_range();
        lines.push(LineBox {
            top: metrics.block_min_coord,
            height: metrics.block_max_coord - metrics.block_min_coord,
            start: bytes.start,
            end: bytes.end,
        });
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let mut x = glyph_run.offset();
            for cluster in glyph_run.run().visual_clusters() {
                graphemes.push(GraphemeBox::placed(
                    cluster.text_range(),
                    x,
                    cluster.advance(),
                    index,
                    cluster.is_rtl(),
                ));
                x += cluster.advance();
            }
        }
    }
    if lines.is_empty() {
        // No line at all (an empty text, for some engines): the caret's line
        // is one of the style's height.
        lines.push(LineBox {
            top: 0.0,
            height: style.size * style.line_height,
            start: 0,
            end: text.len(),
        });
    }
    EditingGeometry::new(text, graphemes, lines)
}

fn family_property(family: &FontFamily) -> parley::FontFamily<'static> {
    match family {
        FontFamily::SansSerif => GenericFamily::SansSerif.into(),
        FontFamily::Serif => GenericFamily::Serif.into(),
        FontFamily::Monospace => GenericFamily::Monospace.into(),
        FontFamily::Named(name) => parley::FontFamily::named(name).into_owned(),
    }
}

impl TextEngine for ParleyEngine {
    fn register_font(&mut self, data: Vec<u8>) -> Vec<String> {
        let registered = self
            .font_context
            .collection
            .register_fonts(Blob::from(data), None);
        let families: Vec<_> = registered.iter().map(|(family, _)| *family).collect();
        if !self.sees_system_fonts {
            for generic in GENERIC_FAMILIES {
                self.font_context
                    .collection
                    .append_generic_families(generic, families.iter().copied());
            }
        }
        families
            .into_iter()
            .filter_map(|family| {
                self.font_context
                    .collection
                    .family_name(family)
                    .map(str::to_string)
            })
            .collect()
    }

    fn shape(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
        shaping: Shaping,
    ) -> LayoutData {
        // Scale 1 and NO quantisation: positions stay fractional dp, so the
        // renderer can place them on any device grid (the module's header).
        let mut builder =
            self.layout_context
                .ranged_builder(&mut self.font_context, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(family_property(&style.family)));
        builder.push_default(StyleProperty::FontSize(style.size));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            style.line_height,
        )));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(style.weight)));
        builder.push_default(StyleProperty::FontStyle(if style.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        }));
        if shaping == Shaping::EditedBreakingWords {
            builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::BreakWord));
        }
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(max_width);
        layout.align(Alignment::Start, AlignmentOptions::default());

        let mut runs = Vec::new();
        let mut line_count = 0;
        for line in layout.lines() {
            line_count += 1;
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let font = self.intern(run);
                let glyphs = glyph_run
                    .positioned_glyphs()
                    .map(|glyph| ShapedGlyph {
                        id: glyph.id,
                        x: glyph.x,
                        y: glyph.y,
                    })
                    .collect();
                runs.push(ShapedRun {
                    font,
                    font_size: run.font_size(),
                    glyphs,
                });
            }
        }
        LayoutData {
            system: 0,
            size: Size::new(layout.width(), layout.height()),
            line_count,
            first_baseline: layout.first_baseline().unwrap_or(0.0),
            runs,
            editing: (shaping != Shaping::Plain).then(|| editing_geometry(&layout, text, style)),
        }
    }

    fn rasterize(
        &mut self,
        font: u32,
        glyph: u32,
        size: f32,
        subpixel_offset: f32,
    ) -> Option<RasterizedGlyph> {
        let instance = self.instances.get(usize::try_from(font).ok()?)?;
        let glyph = u16::try_from(glyph).ok()?;
        let face =
            swash::FontRef::from_index(instance.data.data.data(), instance.data.index as usize)?;
        // Unhinted: hinting would snap outlines to a grid per size and break
        // the linear scaling the dp layout relies on.
        let mut scaler = self
            .scale_context
            .builder(face)
            .size(size)
            .hint(false)
            .normalized_coords(instance.coordinates.iter())
            .build();
        let mut render = Render::new(&[
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ]);
        render
            .format(Format::Alpha)
            .offset(Vector::new(subpixel_offset, 0.0));
        if instance.embolden {
            // A synthetic bold the family lacks: strokes widened by 1/36 of
            // the size each way, about what a regular-to-bold step adds.
            render.embolden(size / 36.0);
        }
        if let Some(degrees) = instance.skew_degrees {
            render.transform(Some(Transform::skew(
                Angle::from_degrees(degrees),
                Angle::ZERO,
            )));
        }
        let image = render.render(&mut scaler, glyph)?;
        let pixels = (image.placement.width * image.placement.height) as usize;
        let content = match image.content {
            Content::Color => GlyphContent::Color(image.data),
            Content::Mask => GlyphContent::Mask(image.data),
            // Not requested (`Format::Alpha`); read defensively as the green
            // channel's coverage should a source answer one anyway.
            Content::SubpixelMask => {
                let stride = image.data.len() / pixels.max(1);
                GlyphContent::Mask(
                    (0..pixels)
                        .map(|pixel| image.data[pixel * stride + 1.min(stride.saturating_sub(1))])
                        .collect(),
                )
            }
        };
        Some(RasterizedGlyph {
            left: image.placement.left,
            top: image.placement.top,
            width: image.placement.width,
            height: image.placement.height,
            content,
        })
    }
}
