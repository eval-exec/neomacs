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
    ) -> Result<&PopupSurface, String> {
        let parent = match self.surfaces.last() {
            Some(parent) if parent.presented => parent.window.clone(),
            Some(_) => return Err("popup parent has not presented yet".into()),
            None => root,
        };
        let surface = PopupSurface::create(
            event_loop, parent, placement, extent, instance, adapter, device, format,
        )?;
        self.surfaces.push(surface);
        Ok(self.surfaces.last().unwrap())
    }
    pub fn iter(&self) -> impl Iterator<Item = &PopupSurface> {
        self.surfaces.iter()
    }
    pub fn get(&self, depth: usize) -> Option<&PopupSurface> {
        self.surfaces.get(depth)
    }
    pub fn truncate(&mut self, depth: usize) {
        // Vec::truncate does not promise the child-before-parent native order.
        while self.surfaces.len() > depth {
            self.surfaces.pop();
        }
    }
}

impl std::ops::Index<usize> for PopupHost {
    type Output = PopupSurface;
    fn index(&self, depth: usize) -> &Self::Output {
        &self.surfaces[depth]
    }
}
impl std::ops::IndexMut<usize> for PopupHost {
    fn index_mut(&mut self, depth: usize) -> &mut Self::Output {
        &mut self.surfaces[depth]
    }
}
impl Drop for PopupHost {
    fn drop(&mut self) {
        self.truncate(0);
    }
}
