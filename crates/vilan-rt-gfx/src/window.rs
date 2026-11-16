//! Drawing into a `winit` window.
//!
//! This is the renderer's half of a window only: a surface on the window, kept
//! the window's size, and a frame drawn when the caller asks. The EVENT LOOP is
//! the caller's. A frame is drawn on demand: the caller asks for one with
//! `Window::request_redraw` when something changed (a resize, a scale change,
//! a repaint) and calls [`WindowRenderer::render`] on `RedrawRequested`.
//! Nothing here draws on a timer, so an idle window costs nothing.
//!
//! How the window's loop meets the vilan program's executor is native-ui.md's
//! Q8 (RULED: the main thread, after an executor slice); it is not this
//! crate's yet. `examples/window.rs` drives a window standalone.

use std::sync::Arc;

use winit::dpi::PhysicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::GfxError;
use crate::atlas::{AtlasLimits, AtlasStats};
use crate::color::Color;
use crate::gpu::GpuContext;
use crate::renderer::{FrameStats, Renderer, Target};
use crate::scene::Scene;
use crate::text::TextSystem;
use crate::units::Units;

/// A renderer drawing into one window's surface.
pub struct WindowRenderer {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    configuration: wgpu::SurfaceConfiguration,
    context: GpuContext,
    renderer: Renderer,
}

/// What [`WindowRenderer::render`] did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameOutcome {
    /// The frame was drawn and presented.
    Presented(FrameStats),
    /// No frame could be drawn now (the window is minimised or covered, the
    /// surface timed out, or it had to be reconfigured): ask for another
    /// redraw when the window changes.
    Skipped,
}

impl WindowRenderer {
    /// A surface on `window` and a device that can present to it.
    pub fn new(
        event_loop: &ActiveEventLoop,
        window: Arc<Window>,
    ) -> Result<WindowRenderer, GfxError> {
        let mut descriptor = wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(
            event_loop.owned_display_handle(),
        ));
        descriptor.flags = crate::gpu::instance_flags();
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance
            .create_surface(window.clone())
            .map_err(|error| GfxError::Surface(error.to_string()))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            apply_limit_buckets: false,
        }))
        .map_err(|_| GfxError::NoAdapter)?;
        let context = GpuContext::with_adapter(instance, adapter)?;
        let capabilities = surface.get_capabilities(&context.adapter);
        // An sRGB format, so blending happens in linear light as it does
        // offscreen; the first format otherwise.
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| GfxError::Surface("the surface offers no texture format".to_string()))?;
        let size = window.inner_size();
        let configuration = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: capabilities
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&context.device, &configuration);
        let renderer = Renderer::new(&context, format, AtlasLimits::default());
        Ok(WindowRenderer {
            window,
            surface,
            configuration,
            context,
            renderer,
        })
    }

    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    pub fn context(&self) -> &GpuContext {
        &self.context
    }

    pub fn atlas_stats(&self) -> AtlasStats {
        self.renderer.atlas_stats()
    }

    /// The surface's size in device pixels.
    pub fn size(&self) -> PhysicalSize<u32> {
        PhysicalSize::new(self.configuration.width, self.configuration.height)
    }

    /// The window's units now: its current scale factor, and `text_scale`.
    pub fn units(&self, text_scale: f32) -> Result<Units, GfxError> {
        Units::new(self.window.scale_factor() as f32, text_scale)
    }

    /// Follows the window to a new size in device pixels (`Resized`, and the
    /// new size a `ScaleFactorChanged` brings). Does not draw.
    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        let (width, height) = (size.width.max(1), size.height.max(1));
        if (width, height) == (self.configuration.width, self.configuration.height) {
            return;
        }
        self.configuration.width = width;
        self.configuration.height = height;
        self.surface
            .configure(&self.context.device, &self.configuration);
    }

    /// Draws `scene` over `background` and presents it.
    pub fn render(
        &mut self,
        scene: &Scene,
        units: Units,
        text: &mut TextSystem,
        background: Color,
    ) -> Result<FrameOutcome, GfxError> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                // Draw this one; the next frame gets a configuration that
                // matches again.
                self.surface
                    .configure(&self.context.device, &self.configuration);
                frame
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(FrameOutcome::Skipped);
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface
                    .configure(&self.context.device, &self.configuration);
                return Ok(FrameOutcome::Skipped);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self
                    .context
                    .instance
                    .create_surface(self.window.clone())
                    .map_err(|error| GfxError::Surface(error.to_string()))?;
                self.surface
                    .configure(&self.context.device, &self.configuration);
                return Ok(FrameOutcome::Skipped);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(GfxError::Surface(
                    "acquiring the window's next frame failed validation".to_string(),
                ));
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let stats = self.renderer.render(
            &self.context,
            Target {
                view: &view,
                width: self.configuration.width,
                height: self.configuration.height,
            },
            scene,
            units,
            text,
            background,
        )?;
        self.window.pre_present_notify();
        self.context.queue.present(frame);
        Ok(FrameOutcome::Presented(stats))
    }
}
