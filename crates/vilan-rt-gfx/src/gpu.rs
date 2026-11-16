//! The GPU device: an adapter chosen once, a device and its queue.

use crate::GfxError;

/// The instance flags every context is made with: wgpu's defaults for the
/// build, WITHOUT `DEBUG` unless `WGPU_DEBUG=1` asks for it.
///
/// `DEBUG` names every GPU object through `VK_EXT_debug_utils`, which helps a
/// frame debugger and nothing else. With it on, devices used from several
/// threads at once crashed (SIGSEGV) inside the Vulkan loader's
/// `vkSetDebugUtilsObjectNameEXT`: 4 of 12 runs of the golden tests, which
/// each make a device on their own thread (loader 1.4.309, lavapipe, Mesa
/// 25.0.7, 2026-11-10), and 0 of 20 with it off. Release builds never set
/// it, so only debug builds and tests met the crash, but a program may well
/// draw offscreen from more than one thread.
pub(crate) fn instance_flags() -> wgpu::InstanceFlags {
    (wgpu::InstanceFlags::from_build_config() - wgpu::InstanceFlags::DEBUG).with_env()
}

/// Which adapter a headless context takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterPreference {
    /// What the system recommends: the high-performance GPU when there is one.
    HighPerformance,
    /// A software (CPU) adapter only, Vulkan's (lavapipe) before OpenGL's
    /// (llvmpipe). For output that must be the same on every machine that
    /// has one: golden images. Two software drivers still differ by one in a
    /// few channel values, which the golden tests' tolerance absorbs.
    Software,
}

/// What a context runs on, for a log line or a test's report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterSummary {
    pub name: String,
    pub backend: String,
    pub device_type: String,
    pub driver: String,
}

/// An adapter, its device and its queue. Every renderer needs one; a window's
/// comes with its surface ([`WindowRenderer`](crate::WindowRenderer)).
///
/// A clone is the same device (wgpu's handles are counted), so renderers made
/// one after another can share one instead of each paying for a new device.
#[derive(Clone)]
pub struct GpuContext {
    pub(crate) instance: wgpu::Instance,
    pub(crate) adapter: wgpu::Adapter,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
}

impl GpuContext {
    /// A context with no window. `Err(GfxError::NoAdapter)` when the machine
    /// has no adapter the preference accepts (no GPU, no software driver).
    pub fn headless(preference: AdapterPreference) -> Result<GpuContext, GfxError> {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        descriptor.flags = instance_flags();
        let instance = wgpu::Instance::new(descriptor);
        let adapter = match preference {
            AdapterPreference::HighPerformance => {
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: None,
                    apply_limit_buckets: false,
                }))
                .map_err(|_| GfxError::NoAdapter)?
            }
            AdapterPreference::Software => {
                let adapters =
                    pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
                let rank = |adapter: &wgpu::Adapter| match adapter.get_info().backend {
                    wgpu::Backend::Vulkan => 0,
                    wgpu::Backend::Gl => 1,
                    _ => 2,
                };
                adapters
                    .into_iter()
                    .filter(|adapter| adapter.get_info().device_type == wgpu::DeviceType::Cpu)
                    .min_by_key(rank)
                    .ok_or(GfxError::NoAdapter)?
            }
        };
        GpuContext::with_adapter(instance, adapter)
    }

    pub(crate) fn with_adapter(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
    ) -> Result<GpuContext, GfxError> {
        // WebGL2's limits, raised only to the adapter's texture size: what runs
        // here must not need more than the browser part (P6) will have.
        let required_limits =
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("vilan-rt-gfx"),
            required_limits,
            ..Default::default()
        }))
        .map_err(|error| GfxError::Device(error.to_string()))?;
        Ok(GpuContext {
            instance,
            adapter,
            device,
            queue,
        })
    }

    pub fn adapter(&self) -> AdapterSummary {
        let info = self.adapter.get_info();
        AdapterSummary {
            name: info.name,
            backend: format!("{:?}", info.backend),
            device_type: format!("{:?}", info.device_type),
            driver: format!("{} {}", info.driver, info.driver_info)
                .trim()
                .to_string(),
        }
    }

    /// The largest texture side the device accepts (an offscreen target or an
    /// image larger than this is refused).
    pub fn maximum_texture_side(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }
}
