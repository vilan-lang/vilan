//! Drawing into a texture and reading it back: golden-image tests, and later
//! headless rendering (thumbnails, a server that draws).

use crate::GfxError;
use crate::atlas::{AtlasLimits, AtlasStats};
use crate::color::Color;
use crate::gpu::GpuContext;
use crate::renderer::{FrameStats, Renderer, Target};
use crate::scene::Scene;
use crate::text::TextSystem;
use crate::units::Units;

/// The format of an offscreen target: 8-bit sRGB RGBA, so the bytes read back
/// are what a PNG holds.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// A renderer drawing into a texture of its own, of a fixed size in device
/// pixels.
pub struct OffscreenRenderer {
    context: GpuContext,
    renderer: Renderer,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl OffscreenRenderer {
    /// A `width` x `height` device-pixel target. Refuses a zero side or one
    /// past the device's texture limit.
    pub fn new(
        context: GpuContext,
        width: u32,
        height: u32,
    ) -> Result<OffscreenRenderer, GfxError> {
        OffscreenRenderer::with_atlas_limits(context, width, height, AtlasLimits::default())
    }

    pub fn with_atlas_limits(
        context: GpuContext,
        width: u32,
        height: u32,
        atlas_limits: AtlasLimits,
    ) -> Result<OffscreenRenderer, GfxError> {
        let maximum_side = context.maximum_texture_side();
        if width == 0 || height == 0 || width > maximum_side || height > maximum_side {
            return Err(GfxError::InvalidTargetSize {
                width,
                height,
                maximum_side,
            });
        }
        let texture = context.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let renderer = Renderer::new(&context, FORMAT, atlas_limits);
        Ok(OffscreenRenderer {
            context,
            renderer,
            texture,
            view,
            width,
            height,
        })
    }

    pub fn context(&self) -> &GpuContext {
        &self.context
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn atlas_stats(&self) -> AtlasStats {
        self.renderer.atlas_stats()
    }

    /// Draws `scene` over `background`. Returns once the frame is submitted;
    /// [`read_rgba`](OffscreenRenderer::read_rgba) waits for the GPU.
    pub fn render(
        &mut self,
        scene: &Scene,
        units: Units,
        text: &mut TextSystem,
        background: Color,
    ) -> Result<FrameStats, GfxError> {
        self.renderer.render(
            &self.context,
            Target {
                view: &self.view,
                width: self.width,
                height: self.height,
            },
            scene,
            units,
            text,
            background,
        )
    }

    /// Waits until the GPU has finished every frame submitted so far.
    pub fn finish(&self) -> Result<(), GfxError> {
        self.context
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .map(|_| ())
            .map_err(|error| GfxError::Readback(error.to_string()))
    }

    /// The target's pixels: `width * height * 4` bytes, sRGB RGBA, rows top
    /// to bottom. Waits for every submitted frame to finish.
    pub fn read_rgba(&self) -> Result<Vec<u8>, GfxError> {
        let row_bytes = self.width * 4;
        // A buffer copy's rows are aligned to 256 bytes.
        let padded_row_bytes = row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("offscreen readback"),
            size: u64::from(padded_row_bytes) * u64::from(self.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder =
            self.context
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("readback"),
                });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.context.queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            // The receiver outlives the wait below; a send cannot fail there.
            let _ = sender.send(result);
        });
        self.context
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| GfxError::Readback(error.to_string()))?;
        receiver
            .recv()
            .map_err(|error| GfxError::Readback(error.to_string()))?
            .map_err(|error| GfxError::Readback(error.to_string()))?;
        let mapped = slice
            .get_mapped_range()
            .map_err(|error| GfxError::Readback(format!("{error:?}")))?;
        let mut pixels = Vec::with_capacity(row_bytes as usize * self.height as usize);
        for row in 0..self.height as usize {
            let start = row * padded_row_bytes as usize;
            pixels.extend_from_slice(&mapped[start..start + row_bytes as usize]);
        }
        drop(mapped);
        readback.unmap();
        Ok(pixels)
    }
}
