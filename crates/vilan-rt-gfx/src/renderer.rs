//! The GPU half of a frame: pipelines, buffers, atlas pages and image
//! textures, and one render pass per frame.

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use crate::GfxError;
use crate::atlas::{
    AtlasGlyph, AtlasKey, AtlasLimits, AtlasStats, GlyphAtlas, PageKind, PageStorage,
};
use crate::color::Color;
use crate::encode::{
    BatchKind, EncodedFrame, Encoder, GlyphSource, QuadInstance, SpriteInstance, TextureKey,
};
use crate::gpu::GpuContext;
use crate::path::{MaskStorage, PathMaskKey, PathMasks, PlacedMask};
use crate::scene::{ImageData, Scene};
use crate::text::{GlyphRasterKey, TextSystem};
use crate::units::Units;

/// What one frame cost and drew.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameStats {
    pub draw_calls: u32,
    pub quad_instances: u32,
    pub sprite_instances: u32,
    /// Primitives in the scene, nested scenes included.
    pub primitives: u32,
    /// Primitives that drew nothing (outside their clip or the target,
    /// transparent, or empty).
    pub culled_primitives: u32,
    pub atlas: AtlasStats,
    /// Path masks too large for the atlas, each in a texture of its own
    /// (gfx-path.md R8), after the frame.
    pub path_mask_textures: u32,
    /// CPU time to encode the scene, upload, record and submit. Not the GPU's
    /// time: submission returns before the GPU finishes.
    pub cpu_time: Duration,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    viewport: [f32; 2],
    padding: [f32; 2],
}

/// Instance buffers start with room for this many instances and double.
const INITIAL_INSTANCE_CAPACITY: u64 = 1024;

/// A texture the sprite pipeline samples, and its bind group.
struct BoundTexture {
    bind_group: wgpu::BindGroup,
    /// The texture itself: atlas uploads write to it, and holding it keeps it
    /// alive as long as its bind group.
    texture: wgpu::Texture,
}

/// The atlas's pages on the GPU, and the textures of path masks too large
/// for it.
struct GpuPages {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    mask: Vec<BoundTexture>,
    color: Vec<BoundTexture>,
    path_masks: HashMap<u64, BoundTexture>,
}

fn bind_texture(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    texture: wgpu::Texture,
    label: &str,
) -> BoundTexture {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    BoundTexture {
        bind_group,
        texture,
    }
}

impl PageStorage for GpuPages {
    fn create_page(&mut self, kind: PageKind, page: u32, size: u32) {
        let format = match kind {
            PageKind::Mask => wgpu::TextureFormat::R8Unorm,
            PageKind::Color => wgpu::TextureFormat::Rgba8UnormSrgb,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas page"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bound = bind_texture(
            &self.device,
            &self.layout,
            &self.sampler,
            texture,
            "glyph atlas page",
        );
        let pages = match kind {
            PageKind::Mask => &mut self.mask,
            PageKind::Color => &mut self.color,
        };
        debug_assert_eq!(pages.len() as u32, page, "pages are made in order");
        pages.push(bound);
    }

    fn upload(
        &mut self,
        kind: PageKind,
        page: u32,
        origin: (u32, u32),
        extent: (u32, u32),
        pixels: &[u8],
    ) {
        let pages = match kind {
            PageKind::Mask => &self.mask,
            PageKind::Color => &self.color,
        };
        let texture = &pages[page as usize].texture;
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: origin.0,
                    y: origin.1,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(extent.0 * kind.bytes_per_pixel()),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: extent.0,
                height: extent.1,
                depth_or_array_layers: 1,
            },
        );
    }
}

impl MaskStorage for GpuPages {
    fn create_mask_texture(&mut self, id: u64, width: u32, height: u32, pixels: &[u8]) {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("path mask"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width),
                rows_per_image: None,
            },
            size,
        );
        // Sampled texel for texel, as the glyph pages are: the mask is made
        // at the size it is drawn at, on whole device pixels.
        let bound = bind_texture(
            &self.device,
            &self.layout,
            &self.sampler,
            texture,
            "path mask",
        );
        self.path_masks.insert(id, bound);
    }

    fn free_mask_texture(&mut self, id: u64) {
        self.path_masks.remove(&id);
    }
}

/// The encoder's glyph source for one frame: the atlas, rasterising a miss
/// through the frame's text system and uploading it to a GPU page, and the
/// path masks, rasterised by `zeno`.
struct FrameGlyphs<'a> {
    atlas: &'a mut GlyphAtlas,
    text: &'a mut TextSystem,
    pages: &'a mut GpuPages,
    paths: &'a mut PathMasks,
    maximum_texture_side: u32,
}

impl GlyphSource for FrameGlyphs<'_> {
    fn glyph(&mut self, key: GlyphRasterKey) -> Option<AtlasGlyph> {
        let text = &mut *self.text;
        self.atlas
            .resolve(AtlasKey::Glyph(key), || text.rasterize(&key), self.pages)
    }

    fn page_size(&self) -> u32 {
        self.atlas.page_size()
    }

    fn path_mask(&mut self, key: PathMaskKey) -> Option<PlacedMask> {
        self.paths
            .resolve(key, self.atlas, self.pages, self.maximum_texture_side)
    }
}

/// An image's pixels premultiplied by alpha IN LINEAR LIGHT and encoded back
/// to sRGB bytes, for an sRGB texture. The sampler decodes each texel to
/// linear light before it filters, so what it filters is linear premultiplied
/// colour: a stretched image's transparent edge blends into the colour beside
/// it, not into the black that a transparent pixel's colour channels hold
/// (the dark fringe straight-alpha filtering draws).
fn premultiplied(pixels: &[u8]) -> Vec<u8> {
    let to_linear = |byte: u8| {
        let channel = f32::from(byte) / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    let to_srgb_byte = |linear: f32| {
        let encoded = if linear <= 0.003_130_8 {
            linear * 12.92
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        (encoded * 255.0).round().clamp(0.0, 255.0) as u8
    };
    let mut out = Vec::with_capacity(pixels.len());
    for pixel in pixels.as_chunks::<4>().0 {
        let alpha = f32::from(pixel[3]) / 255.0;
        for channel in &pixel[..3] {
            out.push(if pixel[3] == 255 {
                *channel
            } else {
                to_srgb_byte(to_linear(*channel) * alpha)
            });
        }
        out.push(pixel[3]);
    }
    out
}

/// A texture a frame is drawn into, and its size in device pixels.
pub(crate) struct Target<'a> {
    pub(crate) view: &'a wgpu::TextureView,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

struct UploadedImage {
    source: Weak<ImageData>,
    texture: BoundTexture,
}

/// Draws scenes into textures of one format. Owned by an
/// [`OffscreenRenderer`](crate::OffscreenRenderer) or a
/// [`WindowRenderer`](crate::WindowRenderer); one per target, so each keeps
/// its own atlas and image uploads.
pub(crate) struct Renderer {
    quad_pipeline: wgpu::RenderPipeline,
    sprite_pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    image_sampler: wgpu::Sampler,
    quad_buffer: wgpu::Buffer,
    quad_capacity: u64,
    sprite_buffer: wgpu::Buffer,
    sprite_capacity: u64,
    atlas: GlyphAtlas,
    pages: GpuPages,
    path_masks: PathMasks,
    images: HashMap<u64, UploadedImage>,
    encoder: Encoder,
    frame: EncodedFrame,
}

fn instance_buffer(device: &wgpu::Device, label: &str, bytes: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl Renderer {
    pub(crate) fn new(
        context: &GpuContext,
        format: wgpu::TextureFormat,
        atlas_limits: AtlasLimits,
    ) -> Renderer {
        let device = &context.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vilan-rt-gfx shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        // Glyphs are placed on whole device pixels at their rasterised size,
        // so they are sampled texel for texel; images are stretched, so they
        // are filtered.
        let glyph_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyphs"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("images"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let quad_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quads"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });
        let sprite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprites"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let quad_attributes = wgpu::vertex_attr_array![
            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4,
            5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4, 9 => Float32x4
        ];
        let sprite_attributes = wgpu::vertex_attr_array![
            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4,
            4 => Float32x4, 5 => Float32x4, 6 => Float32x4
        ];
        let pipeline = |label: &str,
                        layout: &wgpu::PipelineLayout,
                        vertex: &str,
                        fragment: &str,
                        stride: usize,
                        attributes: &[wgpu::VertexAttribute]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: stride as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes,
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let quad_pipeline = pipeline(
            "quads",
            &quad_layout,
            "quad_vertex",
            "quad_fragment",
            std::mem::size_of::<QuadInstance>(),
            &quad_attributes,
        );
        let sprite_pipeline = pipeline(
            "sprites",
            &sprite_layout,
            "sprite_vertex",
            "sprite_fragment",
            std::mem::size_of::<SpriteInstance>(),
            &sprite_attributes,
        );
        Renderer {
            quad_pipeline,
            sprite_pipeline,
            globals,
            globals_bind_group,
            image_sampler,
            quad_buffer: instance_buffer(
                device,
                "quad instances",
                INITIAL_INSTANCE_CAPACITY * std::mem::size_of::<QuadInstance>() as u64,
            ),
            quad_capacity: INITIAL_INSTANCE_CAPACITY,
            sprite_buffer: instance_buffer(
                device,
                "sprite instances",
                INITIAL_INSTANCE_CAPACITY * std::mem::size_of::<SpriteInstance>() as u64,
            ),
            sprite_capacity: INITIAL_INSTANCE_CAPACITY,
            atlas: GlyphAtlas::new(atlas_limits),
            pages: GpuPages {
                device: context.device.clone(),
                queue: context.queue.clone(),
                layout: texture_layout,
                sampler: glyph_sampler,
                mask: Vec::new(),
                color: Vec::new(),
                path_masks: HashMap::new(),
            },
            path_masks: PathMasks::default(),
            images: HashMap::new(),
            encoder: Encoder::default(),
            frame: EncodedFrame::default(),
        }
    }

    pub(crate) fn atlas_stats(&self) -> AtlasStats {
        self.atlas.stats()
    }

    /// Uploads the frame's images that are not on the GPU yet, and frees the
    /// textures of images no longer alive anywhere.
    fn sync_images(&mut self, context: &GpuContext) -> Result<(), GfxError> {
        self.images
            .retain(|_, uploaded| uploaded.source.strong_count() > 0);
        let maximum_side = context.maximum_texture_side();
        for image in &self.frame.images {
            let data = &image.data;
            if self.images.contains_key(&data.id) {
                continue;
            }
            if data.width > maximum_side || data.height > maximum_side {
                return Err(GfxError::ImageTooLarge {
                    width: data.width,
                    height: data.height,
                    maximum_side,
                });
            }
            let size = wgpu::Extent3d {
                width: data.width,
                height: data.height,
                depth_or_array_layers: 1,
            };
            let texture = context.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("image"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            context.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &premultiplied(&data.pixels),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(data.width * 4),
                    rows_per_image: None,
                },
                size,
            );
            let texture = bind_texture(
                &context.device,
                &self.pages.layout,
                &self.image_sampler,
                texture,
                "image",
            );
            self.images.insert(
                data.id,
                UploadedImage {
                    source: Arc::downgrade(data),
                    texture,
                },
            );
        }
        Ok(())
    }

    fn upload_instances(&mut self, context: &GpuContext) {
        let quads = self.frame.quads.len() as u64;
        if quads > self.quad_capacity {
            self.quad_capacity = quads.next_power_of_two();
            self.quad_buffer = instance_buffer(
                &context.device,
                "quad instances",
                self.quad_capacity * std::mem::size_of::<QuadInstance>() as u64,
            );
        }
        let sprites = self.frame.sprites.len() as u64;
        if sprites > self.sprite_capacity {
            self.sprite_capacity = sprites.next_power_of_two();
            self.sprite_buffer = instance_buffer(
                &context.device,
                "sprite instances",
                self.sprite_capacity * std::mem::size_of::<SpriteInstance>() as u64,
            );
        }
        if quads > 0 {
            context.queue.write_buffer(
                &self.quad_buffer,
                0,
                bytemuck::cast_slice(&self.frame.quads),
            );
        }
        if sprites > 0 {
            context.queue.write_buffer(
                &self.sprite_buffer,
                0,
                bytemuck::cast_slice(&self.frame.sprites),
            );
        }
    }

    /// Draws `scene` into `target`, a texture of this renderer's format,
    /// cleared to `background` first.
    pub(crate) fn render(
        &mut self,
        context: &GpuContext,
        target: Target<'_>,
        scene: &Scene,
        units: Units,
        text: &mut TextSystem,
        background: Color,
    ) -> Result<FrameStats, GfxError> {
        let started = Instant::now();
        let (width, height) = (target.width, target.height);
        self.atlas.begin_frame();
        self.path_masks.begin_frame();
        let mut glyphs = FrameGlyphs {
            atlas: &mut self.atlas,
            text,
            pages: &mut self.pages,
            paths: &mut self.path_masks,
            maximum_texture_side: context.maximum_texture_side(),
        };
        self.encoder
            .encode(scene, units, (width, height), &mut glyphs, &mut self.frame);
        self.path_masks.end_frame(&mut self.pages);
        self.sync_images(context)?;
        self.upload_instances(context);
        context.queue.write_buffer(
            &self.globals,
            0,
            bytemuck::bytes_of(&Globals {
                viewport: [width as f32, height as f32],
                padding: [0.0; 2],
            }),
        );

        let [red, green, blue, alpha] = background.sanitized();
        // The clear colour is written to an sRGB target as LINEAR values.
        let linear = |channel: f32| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        let clear = wgpu::Color {
            r: f64::from(linear(red) * alpha),
            g: f64::from(linear(green) * alpha),
            b: f64::from(linear(blue) * alpha),
            a: f64::from(alpha),
        };
        let mut command_encoder =
            context
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("frame"),
                });
        {
            let mut pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_bind_group(0, &self.globals_bind_group, &[]);
            for batch in &self.frame.batches {
                match batch.kind {
                    BatchKind::Quads => {
                        pass.set_pipeline(&self.quad_pipeline);
                        pass.set_vertex_buffer(0, self.quad_buffer.slice(..));
                    }
                    BatchKind::Sprites(texture) => {
                        let bound = match texture {
                            TextureKey::MaskPage(page) => &self.pages.mask[page as usize],
                            TextureKey::ColorPage(page) => &self.pages.color[page as usize],
                            TextureKey::Image(id) => &self.images[&id].texture,
                            TextureKey::PathMask(id) => &self.pages.path_masks[&id],
                        };
                        pass.set_pipeline(&self.sprite_pipeline);
                        pass.set_bind_group(1, &bound.bind_group, &[]);
                        pass.set_vertex_buffer(0, self.sprite_buffer.slice(..));
                    }
                }
                pass.draw(0..4, batch.start..batch.end);
            }
        }
        context.queue.submit([command_encoder.finish()]);
        Ok(FrameStats {
            draw_calls: self.frame.batches.len() as u32,
            quad_instances: self.frame.quads.len() as u32,
            sprite_instances: self.frame.sprites.len() as u32,
            primitives: self.frame.primitives,
            culled_primitives: self.frame.culled_primitives,
            atlas: self.atlas.stats(),
            path_mask_textures: self.path_masks.own_textures() as u32,
            cpu_time: started.elapsed(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::premultiplied;

    #[test]
    fn an_image_is_premultiplied_in_linear_light_before_upload() {
        let pixels = [
            200, 100, 50, 255, // opaque: unchanged
            255, 255, 255, 0, // transparent: black
            255, 255, 255, 128, // half white: linear 0.502, sRGB 188
        ];
        assert_eq!(
            premultiplied(&pixels),
            vec![200, 100, 50, 255, 0, 0, 0, 0, 188, 188, 188, 128]
        );
    }
}
