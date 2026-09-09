//! Native drawable ownership, independent of menu content.
use std::sync::Arc;
use winit::window::Window;

pub(crate) struct PopupSurface {
    // Drop the GPU surface before the window, and the child before its parent.
    pub surface: wgpu::Surface<'static>,
    pub window: Arc<dyn Window>,
    pub(in crate::presentation) _parent: Arc<dyn Window>,
    pub config: wgpu::SurfaceConfiguration,
    pub presented: bool,
}

impl PopupSurface {
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(device, &self.config);
        self.window.request_redraw();
    }
}
