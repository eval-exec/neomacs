//! Native drawable ownership, independent of menu content.
use std::sync::Arc;
use winit::window::Window;

pub(super) struct PopupSurface {
    // Drop the GPU surface before the window, and the child before its parent.
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) window: Arc<dyn Window>,
    pub(in crate::presentation) _parent: Arc<dyn Window>,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) submitted: bool,
}

impl PopupSurface {
    pub(super) fn geometry(&self) -> neomacs_display_protocol::DrawableSurface {
        use neomacs_display_protocol::{DeviceScale, SurfaceState};
        let scale =
            DeviceScale::new(self.window.scale_factor() as f32).expect("native popup scale");
        let SurfaceState::Drawable(surface) =
            SurfaceState::from_device_size(self.config.width, self.config.height, scale)
                .expect("native popup geometry")
        else {
            unreachable!("configured popup extent is nonzero")
        };
        surface
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(device, &self.config);
        self.window.request_redraw();
    }
}
