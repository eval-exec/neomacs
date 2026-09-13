use super::PopupSurface;

/// One native popup hierarchy. Native role/parent configuration determines
/// stacking; there is deliberately no numeric cross-window z-index.
#[derive(Default)]
pub(crate) struct PopupHost {
    surfaces: Vec<PopupSurface>,
    retired: super::Retirements<PopupSurface>,
}

impl PopupHost {
    pub fn reposition(&self, depth: usize, placement: neomacs_display_protocol::PopupPlacement) {
        if let Some(surface) = self.surfaces.get(depth) {
            surface.reposition(placement);
        }
    }
    pub fn open(
        &mut self,
        commit: &super::PopupCommit<'_>,
        root: std::sync::Arc<dyn winit::window::Window>,
        placement: neomacs_display_protocol::PopupPlacement,
        extent: (f32, f32),
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> Result<Option<neomacs_display_protocol::DrawableSurface>, String> {
        self.open_with_role(
            commit,
            root,
            placement,
            extent,
            instance,
            adapter,
            device,
            format,
            super::PopupRole::Menu,
        )
    }

    pub fn open_with_role(
        &mut self,
        commit: &super::PopupCommit<'_>,
        root: std::sync::Arc<dyn winit::window::Window>,
        placement: neomacs_display_protocol::PopupPlacement,
        extent: (f32, f32),
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        role: super::PopupRole,
    ) -> Result<Option<neomacs_display_protocol::DrawableSurface>, String> {
        let parent = match self.surfaces.last() {
            Some(parent) if parent.submitted => parent.window.clone(),
            // Submit the parent's buffer before creating a child role. Native
            // presentation feedback is not required to advance this hierarchy.
            Some(_) => return Ok(None),
            None => root,
        };
        let surface = PopupSurface::create(
            commit.event_loop(),
            parent,
            placement,
            extent,
            instance,
            adapter,
            device,
            format,
            role,
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

    /// Borrowed parent after its first buffer submission. Callers retain
    /// child-before-parent teardown; this is not a compositor confirmation.
    pub fn submitted_window(
        &self,
        depth: usize,
    ) -> Option<std::sync::Arc<dyn winit::window::Window>> {
        self.surfaces
            .get(depth)
            .filter(|s| s.submitted)
            .map(|s| s.window.clone())
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

    /// Acquire and submit within one borrow of the hierarchy. Callers receive
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
        popup.submitted = true;
    }
    pub fn truncate(&mut self, depth: usize) {
        // Vec::truncate does not promise the child-before-parent native order.
        while self.surfaces.len() > depth {
            self.retired.push(self.surfaces.pop().unwrap());
        }
    }

    /// End-of-event-batch native teardown. Logical detachment happens earlier,
    /// so events for the retired surfaces can no longer resolve to a panel.
    pub fn commit_retirements(&mut self, _commit: &super::PopupCommit<'_>) {
        self.retired.commit();
    }

    pub fn shutdown(&mut self) {
        self.truncate(0);
        self.retired.commit();
    }
}

impl Drop for PopupHost {
    fn drop(&mut self) {
        self.shutdown();
    }
}
