use thiserror::Error;
use wgpu::{CurrentSurfaceTexture, SurfaceCapabilities};
use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

use super::SurfaceExtent;
use super::policy::{preferred_alpha_mode, preferred_format};

std::cfg_select! {
    target_family = "wasm" => {
        use std::rc::Rc;

        /// Shared ownership used for a browser window and its canvas.
        pub type SurfaceWindow = Rc<dyn Window>;
    }
    _ => {
        use std::sync::Arc;

        /// Shared ownership used for a native window.
        pub type SurfaceWindow = Arc<dyn Window>;
    }
}

/// Failure while creating a surface-compatible GPU context.
#[derive(Debug, Error)]
pub enum SurfaceInitError {
    #[error("failed to create the window surface: {0}")]
    CreateSurface(#[from] wgpu::CreateSurfaceError),
    #[error("failed to find a surface-compatible GPU adapter: {0}")]
    RequestAdapter(#[from] wgpu::RequestAdapterError),
    #[error("failed to create the GPU device: {0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),
    #[error("the GPU adapter reported no usable surface configuration")]
    UnsupportedSurface,
}

/// Failure that cannot be recovered within one presentation attempt.
#[derive(Debug, Error)]
pub enum SurfacePresentError {
    #[error("failed to recreate a lost window surface: {0}")]
    RecreateSurface(#[from] wgpu::CreateSurfaceError),
    #[error("wgpu rejected the current surface configuration")]
    Validation,
    #[error("the recreated surface has no usable configuration")]
    UnsupportedSurface,
    #[error(
        "the GPU device reported {0} consecutive lost surfaces; it will not present again \
         without a full device rebuild"
    )]
    DeviceLost(u32),
}

/// Consecutive `CurrentSurfaceTexture::Lost` results escalated to a device
/// loss, matching the desktop render thread's
/// `CONSECUTIVE_SURFACE_LOST_THRESHOLD`.
///
/// A one-off Lost is an ordinary swapchain hiccup and is merely recovered from.
/// A device that keeps answering Lost never presents again without a rebuild,
/// and recreating the surface each time allocates a fresh `wgpu::Surface` per
/// frame while asking the frontend for another redraw -- an unbounded livelock
/// with no diagnostic. Desktop already escalates; this is the shared runtime
/// reaching the same conclusion rather than spinning.
const CONSECUTIVE_SURFACE_LOST_THRESHOLD: u32 = 30;

/// Consecutive-`Lost` counter behind [`CONSECUTIVE_SURFACE_LOST_THRESHOLD`].
///
/// Split out from the runtime because `wgpu::CurrentSurfaceTexture` owns a real
/// swapchain image and cannot be constructed in a test, so the policy would
/// otherwise only be exercised by an actual driver reset.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct SurfaceLostStreak {
    consecutive: u32,
}

impl SurfaceLostStreak {
    /// An acquisition yielded a texture: the device is presenting again.
    pub(super) const fn acquired(&mut self) {
        self.consecutive = 0;
    }

    /// Record a `Lost`. Returns the streak length once it must be escalated to
    /// a device loss rather than recovered from again.
    pub(super) const fn lost(&mut self) -> Option<u32> {
        self.consecutive += 1;
        if self.consecutive >= CONSECUTIVE_SURFACE_LOST_THRESHOLD {
            Some(self.consecutive)
        } else {
            None
        }
    }
}

/// A non-fatal reason why no frame was presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationSkipReason {
    /// The host currently reports a zero-sized drawable area.
    Suspended,
    /// Acquiring the swapchain image timed out; a later redraw may succeed.
    Timeout,
    /// The host reports that the surface is not currently visible.
    Occluded,
    /// Surface recovery did not settle within the current redraw callback.
    SurfaceChanged,
}

/// Result of one presentation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationOutcome {
    Presented,
    Skipped(PresentationSkipReason),
}

impl PresentationOutcome {
    /// Whether the frontend should immediately schedule another redraw.
    pub const fn should_request_redraw(self) -> bool {
        matches!(
            self,
            Self::Skipped(PresentationSkipReason::Timeout | PresentationSkipReason::SurfaceChanged)
        )
    }
}

/// Linear RGBA color used when a frontend presents without a scene renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceClearColor {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

impl SurfaceClearColor {
    pub const fn rgb(red: f64, green: f64, blue: f64) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 1.0,
        }
    }

    const fn as_wgpu(self) -> wgpu::Color {
        wgpu::Color {
            r: self.red,
            g: self.green,
            b: self.blue,
            a: self.alpha,
        }
    }
}

enum AcquiredFrame {
    Present {
        texture: wgpu::SurfaceTexture,
        reconfigure_after_present: bool,
    },
    Skip(PresentationSkipReason),
}

/// Owns the cross-platform wgpu objects tied to one host window surface.
///
/// Product adapters own the event loop and window. This type owns surface
/// configuration and recovery, while concrete renderers remain free to submit
/// commands to the exposed device and queue before the frame is presented.
pub struct SurfaceRuntime {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    window: SurfaceWindow,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    extent: SurfaceExtent,
    surface_lost: SurfaceLostStreak,
}

impl SurfaceRuntime {
    /// Create a surface-compatible adapter and device for `window`.
    pub async fn new(
        display: OwnedDisplayHandle,
        window: SurfaceWindow,
    ) -> Result<Self, SurfaceInitError> {
        let descriptor = wgpu::InstanceDescriptor::new_with_display_handle(Box::new(display));
        let instance = std::cfg_select! {
            target_family = "wasm" => {
                wgpu::util::new_instance_with_webgpu_detection(descriptor).await
            }
            _ => { wgpu::Instance::new(descriptor) }
        };
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await?;
        let required_limits = std::cfg_select! {
            target_family = "wasm" => {
                browser_device_limits(adapter.get_info().backend, adapter.limits())
            }
            _ => { wgpu::Limits::default() }
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Neomacs portable surface device"),
                required_features: wgpu::Features::empty(),
                required_limits,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await?;
        let size = window.surface_size();
        let extent = SurfaceExtent::from_physical_size(size.width, size.height);
        let config = Self::surface_configuration(&surface.get_capabilities(&adapter), extent)
            .ok_or(SurfaceInitError::UnsupportedSurface)?;

        if extent != SurfaceExtent::Suspended {
            surface.configure(&device, &config);
        }

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            window,
            surface,
            config,
            extent,
            surface_lost: SurfaceLostStreak::default(),
        })
    }

    /// Current physical drawable state.
    pub const fn extent(&self) -> SurfaceExtent {
        self.extent
    }

    /// Device shared with the concrete scene renderer.
    pub fn device(&self) -> wgpu::Device {
        self.device.clone()
    }

    /// Queue shared with the concrete scene renderer.
    pub fn queue(&self) -> wgpu::Queue {
        self.queue.clone()
    }

    /// Texture format selected for the host surface.
    pub const fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Apply a physical resize, suspending configuration for a zero dimension.
    pub fn resize_physical(&mut self, width: u32, height: u32) {
        self.resize(SurfaceExtent::from_physical_size(width, height));
    }

    /// Apply a typed surface extent.
    pub fn resize(&mut self, extent: SurfaceExtent) {
        self.extent = extent;
        let Some((width, height)) = extent.dimensions() else {
            return;
        };

        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    /// Present one frame rendered by `render` into the acquired texture view.
    pub fn present(
        &mut self,
        render: impl FnOnce(&wgpu::TextureView),
    ) -> Result<PresentationOutcome, SurfacePresentError> {
        if self.extent == SurfaceExtent::Suspended {
            return Ok(PresentationOutcome::Skipped(
                PresentationSkipReason::Suspended,
            ));
        }

        let (texture, reconfigure_after_present) = match self.acquire_frame()? {
            AcquiredFrame::Present {
                texture,
                reconfigure_after_present,
            } => (texture, reconfigure_after_present),
            AcquiredFrame::Skip(reason) => return Ok(PresentationOutcome::Skipped(reason)),
        };

        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        render(&view);
        self.queue.present(texture);

        if reconfigure_after_present {
            self.configure_drawable();
        }

        Ok(PresentationOutcome::Presented)
    }

    /// Clear and present a frame without constructing a concrete scene renderer.
    pub fn present_clear(
        &mut self,
        color: SurfaceClearColor,
    ) -> Result<PresentationOutcome, SurfacePresentError> {
        let device = self.device.clone();
        let queue = self.queue.clone();
        let color = color.as_wgpu();
        self.present(move |view| {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Neomacs portable surface clear encoder"),
            });
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Neomacs portable surface clear pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(color),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            }
            queue.submit(std::iter::once(encoder.finish()));
        })
    }

    fn surface_configuration(
        capabilities: &SurfaceCapabilities,
        extent: SurfaceExtent,
    ) -> Option<wgpu::SurfaceConfiguration> {
        let (width, height) = extent.dimensions().unwrap_or((1, 1));
        Some(wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: preferred_format(capabilities)?,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: preferred_alpha_mode(capabilities)?,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        })
    }

    fn configure_drawable(&self) {
        if self.extent != SurfaceExtent::Suspended {
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn recreate_surface(&mut self) -> Result<(), SurfacePresentError> {
        let surface = self.instance.create_surface(self.window.clone())?;
        let capabilities = surface.get_capabilities(&self.adapter);
        let config = Self::surface_configuration(&capabilities, self.extent)
            .ok_or(SurfacePresentError::UnsupportedSurface)?;
        if self.extent != SurfaceExtent::Suspended {
            surface.configure(&self.device, &config);
        }
        self.surface = surface;
        self.config = config;
        Ok(())
    }

    fn acquire_frame(&mut self) -> Result<AcquiredFrame, SurfacePresentError> {
        for _ in 0..2 {
            match self.surface.get_current_texture() {
                CurrentSurfaceTexture::Success(texture) => {
                    self.surface_lost.acquired();
                    return Ok(AcquiredFrame::Present {
                        texture,
                        reconfigure_after_present: false,
                    });
                }
                CurrentSurfaceTexture::Suboptimal(texture) => {
                    self.surface_lost.acquired();
                    return Ok(AcquiredFrame::Present {
                        texture,
                        reconfigure_after_present: true,
                    });
                }
                CurrentSurfaceTexture::Timeout => {
                    return Ok(AcquiredFrame::Skip(PresentationSkipReason::Timeout));
                }
                CurrentSurfaceTexture::Occluded => {
                    return Ok(AcquiredFrame::Skip(PresentationSkipReason::Occluded));
                }
                CurrentSurfaceTexture::Outdated => self.configure_drawable(),
                CurrentSurfaceTexture::Lost => {
                    if let Some(streak) = self.surface_lost.lost() {
                        return Err(SurfacePresentError::DeviceLost(streak));
                    }
                    self.recreate_surface()?;
                }
                CurrentSurfaceTexture::Validation => {
                    return Err(SurfacePresentError::Validation);
                }
            }
        }

        Ok(AcquiredFrame::Skip(PresentationSkipReason::SurfaceChanged))
    }
}

#[cfg(any(target_family = "wasm", test))]
fn browser_device_limits(backend: wgpu::Backend, adapter_limits: wgpu::Limits) -> wgpu::Limits {
    let portable_limits = match backend {
        wgpu::Backend::Gl => wgpu::Limits::downlevel_webgl2_defaults(),
        _ => wgpu::Limits::default(),
    };
    portable_limits.using_resolution(adapter_limits)
}

#[cfg(test)]
mod tests {
    use super::browser_device_limits;

    #[test]
    fn webgl_adapter_keeps_its_supported_surface_resolution() {
        let adapter_limits = wgpu::Limits {
            max_texture_dimension_1d: 8_192,
            max_texture_dimension_2d: 8_192,
            max_texture_dimension_3d: 2_048,
            ..wgpu::Limits::default()
        };

        assert_eq!(
            browser_device_limits(wgpu::Backend::Gl, adapter_limits.clone()),
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter_limits)
        );
    }

    #[test]
    fn webgpu_adapter_keeps_standard_limits_and_adapter_resolution() {
        let adapter_limits = wgpu::Limits {
            max_texture_dimension_1d: 16_384,
            max_texture_dimension_2d: 16_384,
            max_texture_dimension_3d: 4_096,
            ..wgpu::Limits::default()
        };

        assert_eq!(
            browser_device_limits(wgpu::Backend::BrowserWebGpu, adapter_limits.clone()),
            wgpu::Limits::default().using_resolution(adapter_limits)
        );
    }
}

#[cfg(test)]
mod surface_lost_tests {
    use super::{CONSECUTIVE_SURFACE_LOST_THRESHOLD, SurfaceLostStreak};

    #[test]
    fn a_one_off_lost_surface_is_recovered_not_escalated() {
        // The common case: a swapchain hiccup. Escalating here would rebuild
        // the GPU device over a transient.
        let mut streak = SurfaceLostStreak::default();
        assert_eq!(streak.lost(), None);
    }

    #[test]
    fn a_device_that_keeps_answering_lost_escalates_instead_of_looping() {
        // The bug this policy exists for: without it, every attempt recreated
        // a `wgpu::Surface` and asked the frontend for another redraw, so a
        // driver reset became an unbounded livelock with no diagnostic.
        let mut streak = SurfaceLostStreak::default();
        for attempt in 1..CONSECUTIVE_SURFACE_LOST_THRESHOLD {
            assert_eq!(streak.lost(), None, "attempt {attempt} must still recover");
        }
        assert_eq!(
            streak.lost(),
            Some(CONSECUTIVE_SURFACE_LOST_THRESHOLD),
            "the threshold itself must escalate, not the one after it"
        );
    }

    #[test]
    fn only_consecutive_losses_count() {
        // A device that presents between hiccups is healthy; the streak must
        // not accumulate across successful frames into a spurious rebuild.
        let mut streak = SurfaceLostStreak::default();
        for _ in 0..(CONSECUTIVE_SURFACE_LOST_THRESHOLD * 3) {
            assert_eq!(streak.lost(), None);
            streak.acquired();
        }
    }
}
