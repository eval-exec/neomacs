use std::sync::Arc;

use thiserror::Error;
use wgpu::{CurrentSurfaceTexture, SurfaceCapabilities};
use winit::window::Window;

use super::SurfaceExtent;
use super::policy::{preferred_alpha_mode, preferred_format};

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
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    extent: SurfaceExtent,
}

impl SurfaceRuntime {
    /// Create a surface-compatible adapter and device for `window`.
    pub async fn new(window: Arc<Window>) -> Result<Self, SurfaceInitError> {
        let descriptor =
            wgpu::InstanceDescriptor::new_with_display_handle(Box::new(window.clone()));
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Neomacs portable surface device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await?;
        let device = Arc::new(device);
        let queue = Arc::new(queue);
        let size = window.inner_size();
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
        })
    }

    /// Current physical drawable state.
    pub const fn extent(&self) -> SurfaceExtent {
        self.extent
    }

    /// Device shared with the concrete scene renderer.
    pub fn device(&self) -> Arc<wgpu::Device> {
        Arc::clone(&self.device)
    }

    /// Queue shared with the concrete scene renderer.
    pub fn queue(&self) -> Arc<wgpu::Queue> {
        Arc::clone(&self.queue)
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
        color: wgpu::Color,
    ) -> Result<PresentationOutcome, SurfacePresentError> {
        let device = Arc::clone(&self.device);
        let queue = Arc::clone(&self.queue);
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
                    return Ok(AcquiredFrame::Present {
                        texture,
                        reconfigure_after_present: false,
                    });
                }
                CurrentSurfaceTexture::Suboptimal(texture) => {
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
                CurrentSurfaceTexture::Lost => self.recreate_surface()?,
                CurrentSurfaceTexture::Validation => {
                    return Err(SurfacePresentError::Validation);
                }
            }
        }

        Ok(AcquiredFrame::Skip(PresentationSkipReason::SurfaceChanged))
    }
}
