//! From a scene in dp to instance data and draw calls in device pixels: the
//! CPU half of a frame, with no GPU in it (so it is unit-tested without one).
//!
//! 1. **Flatten.** The scene's commands are walked with a stack of what is in
//!    force (transform, clip, opacity); nested scenes are walked in place.
//!    Every primitive is scaled by [`Units`], snapped (`units.rs`), clipped,
//!    and dropped when nothing of it is visible.
//! 2. **Order** (R14). Each surviving primitive's quad bounds go through the
//!    overlap test (`order.rs`), which gives its order. A text run is ONE
//!    primitive: its bounds are the union of its glyphs' quads.
//! 3. **Batch.** Instances are grouped by what a draw call can share: every
//!    SDF quad (rectangles, shadows, underlines) shares one pipeline; textured
//!    quads (glyphs, images) share another and split by texture. Within a
//!    group they are sorted by order, and groups are drained in order: the
//!    group with the lowest pending order emits every instance up to the
//!    lowest order pending in any other group. A frame whose primitives do
//!    not overlap across kinds is therefore one draw for all its rectangles
//!    and one per glyph page, however they are interleaved in the scene.

use crate::atlas::{AtlasGlyph, PageKind};
use crate::color::{Color, Fill};
use crate::geometry::{Corners, Point, Rect, Transform};
use crate::order::{DeviceBounds, OrderAssigner, intersection, is_empty};
use crate::scene::{Command, Image, Rectangle, Scene, Shadow, Underline};
use crate::text::{GlyphRasterKey, TextLayout, subpixel_split};
use crate::units::Units;

/// One SDF quad: a rounded rectangle with fill and border, or a shadow.
/// Every length is in device pixels.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct QuadInstance {
    /// The shape: left, top, width, height.
    pub(crate) bounds: [f32; 4],
    /// Top-left, top-right, bottom-right, bottom-left.
    pub(crate) corner_radii: [f32; 4],
    /// Top, right, bottom, left.
    pub(crate) border_widths: [f32; 4],
    /// sRGB, straight alpha, layer opacity already multiplied in.
    pub(crate) fill_start: [f32; 4],
    pub(crate) fill_end: [f32; 4],
    pub(crate) border_color: [f32; 4],
    /// The gradient's start and end points.
    pub(crate) gradient: [f32; 4],
    /// Left, top, right, bottom.
    pub(crate) clip_bounds: [f32; 4],
    pub(crate) clip_radii: [f32; 4],
    /// `[QUAD_KIND_*, FILL_*, shadow sigma, 0]`.
    pub(crate) parameters: [f32; 4],
}

pub(crate) const QUAD_KIND_RECTANGLE: f32 = 0.0;
pub(crate) const QUAD_KIND_SHADOW: f32 = 1.0;
pub(crate) const FILL_SOLID: f32 = 0.0;
pub(crate) const FILL_LINEAR_GRADIENT: f32 = 1.0;

/// One textured quad: a glyph or an image.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SpriteInstance {
    pub(crate) bounds: [f32; 4],
    /// Normalised texture coordinates: left, top, right, bottom.
    pub(crate) texture_bounds: [f32; 4],
    /// A mask glyph's tint; for a colour glyph or an image, white with the
    /// layer opacity as alpha.
    pub(crate) color: [f32; 4],
    pub(crate) corner_radii: [f32; 4],
    pub(crate) clip_bounds: [f32; 4],
    pub(crate) clip_radii: [f32; 4],
    /// `[SPRITE_KIND_*, 0, 0, 0]`.
    pub(crate) parameters: [f32; 4],
}

pub(crate) const SPRITE_KIND_MASK: f32 = 0.0;
pub(crate) const SPRITE_KIND_COLOR: f32 = 1.0;
pub(crate) const SPRITE_KIND_IMAGE: f32 = 2.0;

/// The texture a textured quad samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum TextureKey {
    MaskPage(u32),
    ColorPage(u32),
    Image(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum BatchKind {
    Quads,
    Sprites(TextureKey),
}

/// One draw call: a run of instances of one kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Batch {
    pub(crate) kind: BatchKind,
    pub(crate) start: u32,
    pub(crate) end: u32,
}

/// Where the encoder finds glyphs: the renderer's atlas (rasterising misses
/// through the frame's text system), or a stand-in in the unit tests.
pub(crate) trait GlyphSource {
    fn glyph(&mut self, key: GlyphRasterKey) -> Option<AtlasGlyph>;
    fn page_size(&self) -> u32;
}

/// What one frame draws, ready to upload.
#[derive(Default)]
pub(crate) struct EncodedFrame {
    pub(crate) quads: Vec<QuadInstance>,
    pub(crate) sprites: Vec<SpriteInstance>,
    pub(crate) batches: Vec<Batch>,
    /// Every image the frame draws, once each, for the renderer to upload.
    pub(crate) images: Vec<Image>,
    pub(crate) primitives: u32,
    pub(crate) culled_primitives: u32,
    /// Nested scenes skipped whole, their primitives counted as culled.
    pub(crate) culled_scenes: u32,
}

#[derive(Clone, Copy)]
struct Clip {
    bounds: DeviceBounds,
    radii: [f32; 4],
}

#[derive(Clone, Copy)]
struct State {
    transform: Transform,
    clip: Clip,
    opacity: f32,
}

/// The reusable buffers of the CPU half of a frame.
pub(crate) struct Encoder {
    assigner: OrderAssigner,
    staged_quads: Vec<(u32, QuadInstance)>,
    staged_sprites: Vec<(TextureKey, u32, SpriteInstance)>,
    glyph_scratch: Vec<(TextureKey, SpriteInstance)>,
    stack: Vec<State>,
}

impl Default for Encoder {
    fn default() -> Self {
        Encoder {
            assigner: OrderAssigner::new(1, 1),
            staged_quads: Vec::new(),
            staged_sprites: Vec::new(),
            glyph_scratch: Vec::new(),
            stack: Vec::new(),
        }
    }
}

fn premultiplied_opacity(color: Color, opacity: f32) -> [f32; 4] {
    color.with_alpha_multiplied(opacity).sanitized()
}

impl Encoder {
    /// Encodes `scene` for a target of `width` x `height` device pixels.
    pub(crate) fn encode(
        &mut self,
        scene: &Scene,
        units: Units,
        (width, height): (u32, u32),
        glyphs: &mut dyn GlyphSource,
        frame: &mut EncodedFrame,
    ) {
        frame.quads.clear();
        frame.sprites.clear();
        frame.batches.clear();
        frame.images.clear();
        frame.primitives = 0;
        frame.culled_primitives = 0;
        frame.culled_scenes = 0;
        self.assigner.reset(width, height);
        self.staged_quads.clear();
        self.staged_sprites.clear();
        self.stack.clear();
        let viewport = Clip {
            bounds: [0.0, 0.0, width as f32, height as f32],
            radii: [0.0; 4],
        };
        let mut walk = Walk {
            encoder: self,
            units,
            glyphs,
            frame,
            state: State {
                transform: Transform::IDENTITY,
                clip: viewport,
                opacity: 1.0,
            },
        };
        walk.scene(scene);
        self.batch(frame);
    }

    fn batch(&mut self, frame: &mut EncodedFrame) {
        // Stable sorts: equal orders keep paint order (not required for
        // correctness, but it keeps output reproducible and readable).
        self.staged_quads.sort_by_key(|(order, _)| *order);
        self.staged_sprites
            .sort_by_key(|(key, order, _)| (*key, *order));

        // The groups, each a sorted run of the staged lists.
        struct Group {
            kind: BatchKind,
            next: usize,
            end: usize,
        }
        let mut groups: Vec<Group> = Vec::new();
        if !self.staged_quads.is_empty() {
            groups.push(Group {
                kind: BatchKind::Quads,
                next: 0,
                end: self.staged_quads.len(),
            });
        }
        let mut start = 0;
        while start < self.staged_sprites.len() {
            let key = self.staged_sprites[start].0;
            let end = start
                + self.staged_sprites[start..]
                    .iter()
                    .take_while(|(other, _, _)| *other == key)
                    .count();
            groups.push(Group {
                kind: BatchKind::Sprites(key),
                next: start,
                end,
            });
            start = end;
        }

        let order_at = |encoder: &Encoder, group: &Group| -> u32 {
            match group.kind {
                BatchKind::Quads => encoder.staged_quads[group.next].0,
                BatchKind::Sprites(_) => encoder.staged_sprites[group.next].1,
            }
        };
        loop {
            // The group with the lowest pending order (ties: the first
            // group, which keeps the output deterministic).
            let mut chosen: Option<(usize, u32)> = None;
            for (index, group) in groups.iter().enumerate() {
                if group.next < group.end {
                    let order = order_at(self, group);
                    if chosen.is_none_or(|(_, lowest)| order < lowest) {
                        chosen = Some((index, order));
                    }
                }
            }
            let Some((chosen_index, _)) = chosen else {
                break;
            };
            let limit = groups
                .iter()
                .enumerate()
                .filter(|(index, group)| *index != chosen_index && group.next < group.end)
                .map(|(_, group)| order_at(self, group))
                .min()
                .unwrap_or(u32::MAX);
            let group = &mut groups[chosen_index];
            let batch_start;
            match group.kind {
                BatchKind::Quads => {
                    batch_start = frame.quads.len() as u32;
                    while group.next < group.end && self.staged_quads[group.next].0 <= limit {
                        frame.quads.push(self.staged_quads[group.next].1);
                        group.next += 1;
                    }
                }
                BatchKind::Sprites(_) => {
                    batch_start = frame.sprites.len() as u32;
                    while group.next < group.end && self.staged_sprites[group.next].1 <= limit {
                        frame.sprites.push(self.staged_sprites[group.next].2);
                        group.next += 1;
                    }
                }
            }
            let batch_end = match group.kind {
                BatchKind::Quads => frame.quads.len() as u32,
                BatchKind::Sprites(_) => frame.sprites.len() as u32,
            };
            match frame.batches.last_mut() {
                Some(last) if last.kind == group.kind && last.end == batch_start => {
                    last.end = batch_end
                }
                _ => frame.batches.push(Batch {
                    kind: group.kind,
                    start: batch_start,
                    end: batch_end,
                }),
            }
        }
    }
}

struct Walk<'a> {
    encoder: &'a mut Encoder,
    units: Units,
    glyphs: &'a mut dyn GlyphSource,
    frame: &'a mut EncodedFrame,
    state: State,
}

impl Walk<'_> {
    fn scene(&mut self, scene: &Scene) {
        for command in scene.commands.iter() {
            match command {
                Command::Rectangle(rectangle) => self.rectangle(rectangle),
                Command::Shadow(shadow) => self.shadow(shadow),
                Command::Text {
                    layout,
                    origin,
                    color,
                } => self.text(layout, *origin, *color),
                Command::Underline(underline) => self.underline(underline),
                Command::Image {
                    image,
                    bounds,
                    corner_radii,
                } => self.image(image, *bounds, *corner_radii),
                Command::PushClip {
                    bounds,
                    corner_radii,
                } => {
                    self.encoder.stack.push(self.state);
                    self.state.clip = self.clip_within(*bounds, *corner_radii);
                }
                Command::PushTransform(transform) => {
                    self.encoder.stack.push(self.state);
                    self.state.transform = self.state.transform.then(*transform);
                }
                Command::PushLayer { opacity } => {
                    self.encoder.stack.push(self.state);
                    self.state.opacity *= opacity;
                }
                Command::Pop => {
                    // A built scene is balanced (`SceneBuilder::finish`), so
                    // every pop has its push.
                    self.state = self.encoder.stack.pop().expect("a built scene is balanced");
                }
                Command::Scene(nested) => {
                    // A nested scene wholly outside the clip draws nothing:
                    // skipped without walking it (a row scrolled out of view
                    // in a long list).
                    if self.outside_clip(nested) {
                        self.frame.primitives += nested.primitive_count() as u32;
                        self.frame.culled_primitives += nested.primitive_count() as u32;
                        self.frame.culled_scenes += 1;
                        continue;
                    }
                    // A nested scene's own pushes and pops are balanced, so it
                    // leaves the state as it found it.
                    let depth = self.encoder.stack.len();
                    self.scene(nested);
                    debug_assert_eq!(depth, self.encoder.stack.len());
                }
            }
        }
    }

    fn content_scale(&self) -> f32 {
        self.units.content_scale()
    }

    /// A dp length under the current transform, in device pixels (radii,
    /// blur): scaled, not snapped.
    fn device_length(&self, length: f32) -> f32 {
        (length * self.state.transform.scale * self.content_scale()).max(0.0)
    }

    /// A rectangle under the current transform, snapped (`units.rs`).
    fn device_rect(&self, rect: Rect) -> DeviceBounds {
        let transformed = self.state.transform.apply_rect(rect);
        let (left, width) = self
            .units
            .snap_span(transformed.left(), transformed.size.width);
        let (top, height) = self
            .units
            .snap_span(transformed.top(), transformed.size.height);
        [left, top, left + width, top + height]
    }

    fn device_point(&self, point: Point) -> [f32; 2] {
        let transformed = self.state.transform.apply(point);
        let scale = self.content_scale();
        [transformed.x * scale, transformed.y * scale]
    }

    /// A border or hairline width under the current transform (R16: the
    /// scale factor and the transform's scale, not the text scale).
    fn line_width(&self, width: f32) -> f32 {
        self.units.line_width(width * self.state.transform.scale)
    }

    fn clip_within(&self, bounds: Rect, corner_radii: Corners) -> Clip {
        let transformed = self.state.transform.apply_rect(bounds);
        let scale = self.content_scale();
        let new_bounds = [
            (transformed.left() * scale).round(),
            (transformed.top() * scale).round(),
            (transformed.right() * scale).round(),
            (transformed.bottom() * scale).round(),
        ];
        let new_radii = corner_radii
            .to_array()
            .map(|radius| self.device_length(radius));
        let parent = self.state.clip;
        let bounds = intersection(&new_bounds, &parent.bounds);
        // Each corner of the intersection keeps the rounding of whichever
        // clip it is a corner of (the larger when both). A corner made of one
        // clip's edge and the other's is square: exact when one clip lies
        // inside the other near its corners, the case scrolling and overflow
        // make; two rounded clips crossing near a corner are approximated.
        let corner_points = |rect: &DeviceBounds| {
            [
                [rect[0], rect[1]],
                [rect[2], rect[1]],
                [rect[2], rect[3]],
                [rect[0], rect[3]],
            ]
        };
        let (result_corners, new_corners, parent_corners) = (
            corner_points(&bounds),
            corner_points(&new_bounds),
            corner_points(&parent.bounds),
        );
        let mut radii = [0.0; 4];
        for corner in 0..4 {
            if result_corners[corner] == new_corners[corner] {
                radii[corner] = new_radii[corner];
            }
            if result_corners[corner] == parent_corners[corner] {
                radii[corner] = f32::max(radii[corner], parent.radii[corner]);
            }
        }
        Clip { bounds, radii }
    }

    /// Whether every pixel `scene` can draw is outside the clip in force,
    /// by its conservative bounds (one device pixel more on every side, for
    /// rounding).
    fn outside_clip(&self, scene: &Scene) -> bool {
        let Some(bounds) = scene.bounds() else {
            return true;
        };
        let transformed = self.state.transform.apply_rect(bounds);
        let scale = self.content_scale();
        let device = [
            transformed.left() * scale - 1.0,
            transformed.top() * scale - 1.0,
            transformed.right() * scale + 1.0,
            transformed.bottom() * scale + 1.0,
        ];
        is_empty(&intersection(&device, &self.state.clip.bounds))
    }

    /// The visible part of a quad: its bounds within the clip, or `None`.
    fn visible(&self, quad: &DeviceBounds) -> Option<DeviceBounds> {
        let visible = intersection(quad, &self.state.clip.bounds);
        (!is_empty(&visible)).then_some(visible)
    }

    fn cull(&mut self) {
        self.frame.culled_primitives += 1;
    }

    fn push_quad(&mut self, quad_bounds: DeviceBounds, instance: QuadInstance) {
        self.frame.primitives += 1;
        let Some(visible) = self.visible(&quad_bounds) else {
            self.cull();
            return;
        };
        let order = self.encoder.assigner.insert(visible);
        self.encoder.staged_quads.push((order, instance));
    }

    fn quad(&self, shape: DeviceBounds) -> QuadInstance {
        QuadInstance {
            bounds: [shape[0], shape[1], shape[2] - shape[0], shape[3] - shape[1]],
            corner_radii: [0.0; 4],
            border_widths: [0.0; 4],
            fill_start: [0.0; 4],
            fill_end: [0.0; 4],
            border_color: [0.0; 4],
            gradient: [0.0; 4],
            clip_bounds: self.state.clip.bounds,
            clip_radii: self.state.clip.radii,
            parameters: [QUAD_KIND_RECTANGLE, FILL_SOLID, 0.0, 0.0],
        }
    }

    fn rectangle(&mut self, rectangle: &Rectangle) {
        let shape = self.device_rect(rectangle.bounds);
        let opacity = self.state.opacity;
        let mut instance = self.quad(shape);
        let widths = rectangle.border.widths;
        instance.border_widths = [
            self.line_width(widths.top),
            self.line_width(widths.right),
            self.line_width(widths.bottom),
            self.line_width(widths.left),
        ];
        instance.border_color = premultiplied_opacity(rectangle.border.color, opacity);
        instance.corner_radii = rectangle
            .corner_radii
            .to_array()
            .map(|radius| self.device_length(radius));
        let fill_alpha = match rectangle.fill {
            Fill::Solid(color) => {
                instance.fill_start = premultiplied_opacity(color, opacity);
                instance.fill_end = instance.fill_start;
                instance.fill_start[3]
            }
            Fill::LinearGradient(gradient) => {
                instance.fill_start = premultiplied_opacity(gradient.start_color, opacity);
                instance.fill_end = premultiplied_opacity(gradient.end_color, opacity);
                let [start_x, start_y] = self.device_point(gradient.start);
                let [end_x, end_y] = self.device_point(gradient.end);
                instance.gradient = [start_x, start_y, end_x, end_y];
                instance.parameters[1] = FILL_LINEAR_GRADIENT;
                instance.fill_start[3].max(instance.fill_end[3])
            }
        };
        let has_border = instance.border_widths.iter().any(|width| *width > 0.0)
            && instance.border_color[3] > 0.0;
        if is_empty(&shape) || (fill_alpha <= 0.0 && !has_border) {
            self.frame.primitives += 1;
            self.cull();
            return;
        }
        self.push_quad(shape, instance);
    }

    fn shadow(&mut self, shadow: &Shadow) {
        let shape = self.device_rect(shadow.bounds);
        let color = premultiplied_opacity(shadow.color, self.state.opacity);
        if is_empty(&shape) || color[3] <= 0.0 {
            self.frame.primitives += 1;
            self.cull();
            return;
        }
        // The shader floors sigma at half a pixel, so an unblurred shadow
        // still has an antialiased edge.
        let sigma = (self.device_length(shadow.blur) / 2.0).max(0.5);
        let reach = (sigma * 3.0).ceil();
        let mut instance = self.quad(shape);
        instance.fill_start = color;
        instance.fill_end = color;
        instance.corner_radii = [self.device_length(shadow.corner_radius); 4];
        instance.parameters = [QUAD_KIND_SHADOW, FILL_SOLID, sigma, 0.0];
        let quad = [
            shape[0] - reach,
            shape[1] - reach,
            shape[2] + reach,
            shape[3] + reach,
        ];
        self.push_quad(quad, instance);
    }

    fn underline(&mut self, underline: &Underline) {
        let transformed = self.state.transform.apply(underline.origin);
        let width = underline.width * self.state.transform.scale;
        let (left, device_width) = self.units.snap_span(transformed.x, width);
        let top = (transformed.y * self.content_scale()).round();
        let thickness = self.line_width(underline.thickness);
        let shape = [left, top, left + device_width, top + thickness];
        let color = premultiplied_opacity(underline.color, self.state.opacity);
        if is_empty(&shape) || color[3] <= 0.0 {
            self.frame.primitives += 1;
            self.cull();
            return;
        }
        let mut instance = self.quad(shape);
        instance.fill_start = color;
        instance.fill_end = color;
        self.push_quad(shape, instance);
    }

    fn sprite(
        &self,
        bounds: DeviceBounds,
        texture_bounds: [f32; 4],
        color: [f32; 4],
        kind: f32,
    ) -> SpriteInstance {
        SpriteInstance {
            bounds: [
                bounds[0],
                bounds[1],
                bounds[2] - bounds[0],
                bounds[3] - bounds[1],
            ],
            texture_bounds,
            color,
            corner_radii: [0.0; 4],
            clip_bounds: self.state.clip.bounds,
            clip_radii: self.state.clip.radii,
            parameters: [kind, 0.0, 0.0, 0.0],
        }
    }

    fn image(&mut self, image: &Image, bounds: Rect, corner_radii: Corners) {
        self.frame.primitives += 1;
        let shape = self.device_rect(bounds);
        let opacity = self.state.opacity.clamp(0.0, 1.0);
        let Some(visible) = (opacity > 0.0 && !is_empty(&shape))
            .then(|| self.visible(&shape))
            .flatten()
        else {
            self.cull();
            return;
        };
        let mut instance = self.sprite(
            shape,
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, opacity],
            SPRITE_KIND_IMAGE,
        );
        instance.corner_radii = corner_radii
            .to_array()
            .map(|radius| self.device_length(radius));
        let order = self.encoder.assigner.insert(visible);
        let key = TextureKey::Image(image.data.id);
        self.encoder.staged_sprites.push((key, order, instance));
        if !self
            .frame
            .images
            .iter()
            .any(|known| known.data.id == image.data.id)
        {
            self.frame.images.push(image.clone());
        }
    }

    fn text(&mut self, layout: &TextLayout, origin: Point, color: Color) {
        self.frame.primitives += 1;
        let tint = premultiplied_opacity(color, self.state.opacity);
        if self.state.opacity <= 0.0 {
            self.cull();
            return;
        }
        let clip = self.state.clip.bounds;
        let page_size = self.glyphs.page_size() as f32;
        let mut scratch = std::mem::take(&mut self.encoder.glyph_scratch);
        scratch.clear();
        let mut union: Option<DeviceBounds> = None;
        let data = &layout.data;
        for run in &data.runs {
            let device_size = self.device_length(run.font_size);
            if device_size.is_nan() || device_size <= 0.0 {
                continue;
            }
            for glyph in &run.glyphs {
                let [pen_x, pen_y] =
                    self.device_point(Point::new(origin.x + glyph.x, origin.y + glyph.y));
                // Glyphs far outside the clip are skipped before the atlas is
                // asked for them. The margins are generous (a glyph's ink
                // rarely reaches an em from its pen), so this never drops a
                // visible pixel; it saves rasterising the off-screen rows of a
                // long paragraph.
                if pen_x > clip[2] + device_size
                    || pen_x < clip[0] - 4.0 * device_size
                    || pen_y > clip[3] + 2.0 * device_size
                    || pen_y < clip[1] - 2.0 * device_size
                {
                    continue;
                }
                let (whole_x, subpixel_bin) = subpixel_split(pen_x);
                let baseline = pen_y.round();
                let key = GlyphRasterKey {
                    system: data.system,
                    font: run.font,
                    glyph: glyph.id,
                    size_bits: device_size.to_bits(),
                    subpixel_bin,
                };
                let Some(placed) = self.glyphs.glyph(key) else {
                    continue;
                };
                let left = whole_x + placed.left as f32;
                let top = baseline - placed.top as f32;
                let quad = [
                    left,
                    top,
                    left + placed.width as f32,
                    top + placed.height as f32,
                ];
                let texture_bounds = [
                    placed.x as f32 / page_size,
                    placed.y as f32 / page_size,
                    (placed.x + placed.width) as f32 / page_size,
                    (placed.y + placed.height) as f32 / page_size,
                ];
                let (texture, color, kind) = match placed.kind {
                    PageKind::Mask => (TextureKey::MaskPage(placed.page), tint, SPRITE_KIND_MASK),
                    PageKind::Color => (
                        TextureKey::ColorPage(placed.page),
                        [1.0, 1.0, 1.0, self.state.opacity.clamp(0.0, 1.0)],
                        SPRITE_KIND_COLOR,
                    ),
                };
                if (kind == SPRITE_KIND_MASK && tint[3] <= 0.0) || self.visible(&quad).is_none() {
                    continue;
                }
                union = Some(match union {
                    None => quad,
                    Some(bounds) => [
                        bounds[0].min(quad[0]),
                        bounds[1].min(quad[1]),
                        bounds[2].max(quad[2]),
                        bounds[3].max(quad[3]),
                    ],
                });
                scratch.push((texture, self.sprite(quad, texture_bounds, color, kind)));
            }
        }
        match union.and_then(|bounds| self.visible(&bounds)) {
            Some(visible) => {
                let order = self.encoder.assigner.insert(visible);
                for (texture, instance) in scratch.drain(..) {
                    self.encoder.staged_sprites.push((texture, order, instance));
                }
            }
            None => self.cull(),
        }
        self.encoder.glyph_scratch = scratch;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Edges;
    use crate::scene::{Border, SceneBuilder};
    use crate::text::{LayoutData, ShapedGlyph, ShapedRun};
    use crate::{Color, Size};
    use std::sync::Arc;

    /// A glyph source where every glyph is an 8 x 10 mask on page 0, its
    /// bitmap's top-left 8 pixels above the pen, except glyph 99, a colour
    /// glyph on colour page 0.
    struct BoxGlyphs {
        requests: Vec<GlyphRasterKey>,
    }

    impl GlyphSource for BoxGlyphs {
        fn glyph(&mut self, key: GlyphRasterKey) -> Option<AtlasGlyph> {
            self.requests.push(key);
            Some(AtlasGlyph {
                kind: if key.glyph == 99 {
                    PageKind::Color
                } else {
                    PageKind::Mask
                },
                page: 0,
                x: 0,
                y: 0,
                width: 8,
                height: 10,
                left: 0,
                top: 8,
            })
        }

        fn page_size(&self) -> u32 {
            256
        }
    }

    fn layout(glyph_ids: &[u32]) -> TextLayout {
        TextLayout {
            data: Arc::new(LayoutData {
                system: 1,
                size: Size::new(10.0 * glyph_ids.len() as f32, 12.0),
                line_count: 1,
                first_baseline: 9.0,
                editing: None,
                runs: vec![ShapedRun {
                    font: 0,
                    font_size: 12.0,
                    glyphs: glyph_ids
                        .iter()
                        .enumerate()
                        .map(|(index, id)| ShapedGlyph {
                            id: *id,
                            x: index as f32 * 10.0,
                            y: 9.0,
                        })
                        .collect(),
                }],
            }),
        }
    }

    fn encode(scene: &Scene, scale_factor: f32) -> EncodedFrame {
        let mut encoder = Encoder::default();
        let mut frame = EncodedFrame::default();
        let units = Units::new(scale_factor, 1.0).expect("valid units");
        let mut glyphs = BoxGlyphs {
            requests: Vec::new(),
        };
        encoder.encode(scene, units, (400, 300), &mut glyphs, &mut frame);
        frame
    }

    fn solid(x: f32, y: f32, width: f32, height: f32) -> Rectangle {
        Rectangle::new(Rect::new(x, y, width, height), Color::rgb(200, 30, 30))
    }

    /// R14, the batching half: rectangles and text that overlap nothing of
    /// the other kind are ONE draw call per kind, however interleaved.
    #[test]
    fn non_overlapping_primitives_batch_by_kind_whatever_their_interleaving() {
        let mut builder = SceneBuilder::new();
        for row in 0..5 {
            let y = row as f32 * 40.0;
            builder.rectangle(solid(0.0, y, 100.0, 30.0));
            builder.text(&layout(&[1, 2, 3]), Point::new(120.0, y), Color::BLACK);
        }
        let frame = encode(&builder.finish().expect("balanced"), 1.0);
        assert_eq!(frame.batches.len(), 2, "{:?}", frame.batches);
        assert_eq!(frame.quads.len(), 5);
        assert_eq!(frame.sprites.len(), 15);
    }

    /// R14, the ordering half: a rectangle that overlaps earlier text is
    /// drawn after it, in a third draw call, even though rectangles were
    /// already drawn before the text.
    #[test]
    fn a_later_rectangle_over_earlier_text_draws_after_it() {
        let mut builder = SceneBuilder::new();
        builder.rectangle(solid(0.0, 0.0, 400.0, 300.0));
        builder.text(&layout(&[1, 2, 3]), Point::new(10.0, 10.0), Color::BLACK);
        builder.rectangle(solid(15.0, 5.0, 10.0, 20.0));
        let frame = encode(&builder.finish().expect("balanced"), 1.0);
        let kinds: Vec<BatchKind> = frame.batches.iter().map(|batch| batch.kind).collect();
        assert_eq!(
            kinds,
            vec![
                BatchKind::Quads,
                BatchKind::Sprites(TextureKey::MaskPage(0)),
                BatchKind::Quads
            ]
        );
    }

    #[test]
    fn colour_glyphs_and_mask_glyphs_draw_from_their_own_pages() {
        let mut builder = SceneBuilder::new();
        builder.text(&layout(&[1, 99, 2]), Point::new(10.0, 10.0), Color::BLACK);
        let frame = encode(&builder.finish().expect("balanced"), 1.0);
        let kinds: Vec<BatchKind> = frame.batches.iter().map(|batch| batch.kind).collect();
        assert_eq!(
            kinds,
            vec![
                BatchKind::Sprites(TextureKey::MaskPage(0)),
                BatchKind::Sprites(TextureKey::ColorPage(0))
            ]
        );
    }

    /// A row scrolled out of a clipped viewport is skipped WHOLE: its nested
    /// scene is not walked, its primitives count as culled, and a row inside
    /// the clip (or straddling its edge) is drawn. The transform in force is
    /// applied to the nested scene's bounds first (the scroll's offset).
    #[test]
    fn a_nested_scene_wholly_outside_the_clip_is_skipped_whole() {
        let row = |y: f32| {
            let mut row = SceneBuilder::new();
            row.rectangle(solid(0.0, y, 100.0, 20.0));
            row.text(&layout(&[1, 2]), Point::new(4.0, y + 4.0), Color::BLACK);
            row.finish().expect("balanced")
        };
        let mut builder = SceneBuilder::new();
        builder.push_clip(Rect::new(0.0, 0.0, 100.0, 100.0), Corners::ZERO);
        builder.push_transform(Transform::translate(0.0, -200.0));
        for index in 0..20 {
            builder.scene(&row(index as f32 * 20.0));
        }
        builder.pop();
        builder.pop();
        let frame = encode(&builder.finish().expect("balanced"), 1.0);
        // Rows 10 to 14 are inside (y 200 to 300 moved to 0 to 100); the text
        // of row 9 reaches within its conservative bounds, so it is walked
        // (its primitives culled one by one) and so is row 15.
        assert_eq!(frame.culled_scenes, 13, "rows 0-8 and 16-19");
        assert_eq!(frame.primitives, 40);
        assert_eq!(frame.quads.len(), 5, "the five visible rows' rectangles");
    }

    #[test]
    fn a_scenes_bounds_hold_every_primitive_under_its_transforms() {
        let mut builder = SceneBuilder::new();
        builder.rectangle(solid(10.0, 10.0, 10.0, 10.0));
        builder.push_transform(Transform::translate(100.0, 0.0));
        builder.rectangle(solid(0.0, 50.0, 10.0, 10.0));
        builder.pop();
        builder.rectangle(solid(0.0, 0.0, 5.0, 5.0));
        let scene = builder.finish().expect("balanced");
        assert_eq!(scene.bounds(), Some(Rect::new(0.0, 0.0, 110.0, 60.0)));
        let mut outer = SceneBuilder::new();
        outer.push_transform(Transform::translate(0.0, 10.0));
        outer.scene(&scene);
        outer.pop();
        assert_eq!(
            outer.finish().expect("balanced").bounds(),
            Some(Rect::new(0.0, 10.0, 110.0, 60.0))
        );
        assert_eq!(Scene::empty().bounds(), None);
    }

    #[test]
    fn a_primitive_outside_its_clip_is_culled() {
        let mut builder = SceneBuilder::new();
        builder.push_clip(Rect::new(0.0, 0.0, 50.0, 50.0), Corners::ZERO);
        builder.rectangle(solid(60.0, 60.0, 10.0, 10.0));
        builder.rectangle(solid(10.0, 10.0, 10.0, 10.0));
        builder.text(&layout(&[1]), Point::new(200.0, 200.0), Color::BLACK);
        builder.pop();
        builder.rectangle(solid(500.0, 0.0, 10.0, 10.0));
        let frame = encode(&builder.finish().expect("balanced"), 1.0);
        assert_eq!(frame.primitives, 4);
        assert_eq!(frame.culled_primitives, 3);
        assert_eq!(frame.quads.len(), 1);
        assert_eq!(frame.quads[0].clip_bounds, [0.0, 0.0, 50.0, 50.0]);
    }

    #[test]
    fn a_transform_and_a_layer_reach_a_nested_scene() {
        let mut inner = SceneBuilder::new();
        inner.rectangle(solid(0.0, 0.0, 10.0, 10.0));
        let inner = inner.finish().expect("balanced");
        let mut builder = SceneBuilder::new();
        builder.push_transform(Transform::translate(20.0, 30.0));
        builder.push_transform(Transform::scale(2.0));
        builder.push_layer(0.5);
        builder.scene(&inner);
        builder.pop();
        builder.pop();
        builder.pop();
        builder.scene(&inner);
        let frame = encode(&builder.finish().expect("balanced"), 1.0);
        assert_eq!(frame.quads.len(), 2);
        assert_eq!(frame.quads[0].bounds, [20.0, 30.0, 20.0, 20.0]);
        assert_eq!(frame.quads[0].fill_start[3], 0.5);
        assert_eq!(frame.quads[1].bounds, [0.0, 0.0, 10.0, 10.0]);
        assert_eq!(frame.quads[1].fill_start[3], 1.0);
    }

    #[test]
    fn borders_follow_the_hairline_rule_and_radii_scale() {
        let mut builder = SceneBuilder::new();
        builder.rectangle(
            solid(10.0, 10.0, 40.0, 20.0)
                .with_radius(4.0)
                .with_border(Border {
                    widths: Edges {
                        top: 1.0,
                        right: 2.0,
                        bottom: 0.0,
                        left: 3.0,
                    },
                    color: Color::BLACK,
                }),
        );
        let frame = encode(&builder.finish().expect("balanced"), 1.5);
        let quad = frame.quads[0];
        assert_eq!(quad.bounds, [15.0, 15.0, 60.0, 30.0]);
        assert_eq!(quad.border_widths, [1.0, 3.0, 0.0, 5.0]); // 1.5->1, 3->3, 4.5->5
        assert_eq!(quad.corner_radii, [6.0; 4]);
    }

    #[test]
    fn a_glyph_is_asked_for_at_its_device_size_and_subpixel_bin() {
        let mut builder = SceneBuilder::new();
        builder.text(&layout(&[1, 2]), Point::new(0.3, 0.0), Color::BLACK);
        let scene = builder.finish().expect("balanced");
        let mut encoder = Encoder::default();
        let mut frame = EncodedFrame::default();
        let mut glyphs = BoxGlyphs {
            requests: Vec::new(),
        };
        encoder.encode(
            &scene,
            Units::new(1.25, 1.0).expect("valid"),
            (400, 300),
            &mut glyphs,
            &mut frame,
        );
        // pen x: 0.3 * 1.25 = 0.375 (bin 2), (10.3) * 1.25 = 12.875 (bin 4 -> 13, bin 0)
        let bins: Vec<(u32, u8)> = glyphs
            .requests
            .iter()
            .map(|key| (key.glyph, key.subpixel_bin))
            .collect();
        assert_eq!(bins, vec![(1, 2), (2, 0)]);
        assert!(
            glyphs
                .requests
                .iter()
                .all(|key| f32::from_bits(key.size_bits) == 15.0)
        );
        // The baseline (9 * 1.25 = 11.25) snaps to 11; the bitmap's top is 8 above.
        assert_eq!(frame.sprites[1].bounds, [13.0, 3.0, 8.0, 10.0]);
    }

    #[test]
    fn nested_rounded_clips_keep_the_corners_they_share() {
        let mut builder = SceneBuilder::new();
        builder.push_clip(Rect::new(0.0, 0.0, 100.0, 100.0), Corners::all(10.0));
        builder.push_clip(Rect::new(0.0, 50.0, 200.0, 200.0), Corners::all(4.0));
        builder.rectangle(solid(0.0, 0.0, 300.0, 300.0));
        builder.pop();
        builder.pop();
        let frame = encode(&builder.finish().expect("balanced"), 1.0);
        let quad = frame.quads[0];
        assert_eq!(quad.clip_bounds, [0.0, 50.0, 100.0, 100.0]);
        // top-left: the inner clip's corner; top-right: neither's (square);
        // bottom-right: the outer clip's; bottom-left: the outer clip's (10)
        // over the inner clip's edge.
        assert_eq!(quad.clip_radii, [4.0, 0.0, 10.0, 10.0]);
    }
}
