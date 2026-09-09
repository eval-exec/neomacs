use super::PopupSurface;

/// One native popup hierarchy. Native role/parent configuration determines
/// stacking; there is deliberately no numeric cross-window z-index.
#[derive(Default)]
pub(crate) struct PopupHost {
    surfaces: Vec<PopupSurface>,
}

impl PopupHost {
    pub fn open(
        &mut self,
        event_loop: &dyn winit::event_loop::ActiveEventLoop,
        root: std::sync::Arc<dyn winit::window::Window>,
        placement: neomacs_display_protocol::PopupPlacement,
        extent: (f32, f32),
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> Result<Option<neomacs_display_protocol::DrawableSurface>, String> {
        let parent = match self.surfaces.last() {
            Some(parent) if parent.presented => parent.window.clone(),
            // A child can only be created after its parent maps. Defer until
            // the parent's redraw instead of exposing native readiness rules.
            Some(_) => return Ok(None),
            None => root,
        };
        let surface = PopupSurface::create(
            event_loop, parent, placement, extent, instance, adapter, device, format,
        )?;
        let geometry = surface.geometry();
        self.surfaces.push(surface);
        Ok(Some(geometry))
    }
    pub fn depth(&self, id: winit::window::WindowId) -> Option<usize> {
        self.surfaces
            .iter()
            .position(|surface| surface.window.id() == id)
    }
    pub fn geometry(&self, depth: usize) -> Option<neomacs_display_protocol::DrawableSurface> {
        self.surfaces.get(depth).map(PopupSurface::geometry)
    }
    pub fn request_redraw(&self) {
        for surface in &self.surfaces {
            surface.window.request_redraw();
        }
    }
    /// None refreshes the extent after a native scale-factor change.
    pub fn resize(
        &mut self,
        depth: usize,
        device: &wgpu::Device,
        size: Option<winit::dpi::PhysicalSize<u32>>,
    ) -> neomacs_display_protocol::DrawableSurface {
        let surface = &mut self.surfaces[depth];
        let size = size.unwrap_or_else(|| surface.window.surface_size());
        surface.resize(device, size.width, size.height);
        surface.geometry()
    }

    /// Acquire and present within one borrow of the hierarchy. Callers receive
    /// only a drawing target, never a native window or an owned surface image.
    /// The painter must submit its commands before returning.
    pub fn draw(
        &mut self,
        depth: usize,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        paint: impl FnOnce(neomacs_renderer_wgpu::renderer::RenderTarget<'_>),
    ) {
        let popup = &mut self.surfaces[depth];
        let output = match popup.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output)
            | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                popup.surface.configure(device, &popup.config);
                popup.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                popup.window.request_redraw();
                return;
            }
            _ => return,
        };
        let view = output.texture.create_view(&Default::default());
        paint(neomacs_renderer_wgpu::renderer::RenderTarget::new(
            &view,
            popup.geometry(),
        ));
        popup.window.pre_present_notify();
        queue.present(output);
        popup.presented = true;
    }
    pub fn truncate(&mut self, depth: usize) {
        // Vec::truncate does not promise the child-before-parent native order.
        while self.surfaces.len() > depth {
            self.surfaces.pop();
        }
    }
}

impl Drop for PopupHost {
    fn drop(&mut self) {
        self.truncate(0);
    }
}
