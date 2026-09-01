use std::num::NonZeroU32;

use super::allocator::{Allocation, ShelfAllocator};
use super::types::*;

pub(crate) struct AtlasPage<M: GlyphMaterial> {
    pub id: PageId<M>,
    pub texture: wgpu::Texture,
    pub linear_bind_group: wgpu::BindGroup,
    pub nearest_bind_group: wgpu::BindGroup,
    pub allocator: ShelfAllocator,
    pub generation: u32,
    pub pinned_this_frame: bool,
    pub last_used_frame: u64,
}

pub(crate) struct PageAllocResult<M: GlyphMaterial> {
    pub page_id: PageId<M>,
    pub generation: u32,
    pub allocation: Allocation,
}

pub(crate) struct GlyphAtlasPages {
    pub alpha: Vec<AtlasPage<AlphaMask>>,
    pub subpixel: Vec<AtlasPage<SubpixelMask>>,
    pub color: Vec<AtlasPage<ColorRgba>>,
    next_page_id: u32,
    config: GlyphAtlasConfig,
}

impl GlyphAtlasPages {
    pub fn new(config: GlyphAtlasConfig) -> Self {
        Self {
            alpha: Vec::new(),
            subpixel: Vec::new(),
            color: Vec::new(),
            next_page_id: 1,
            config,
        }
    }

    pub fn clear(&mut self) {
        self.alpha.clear();
        self.subpixel.clear();
        self.color.clear();
        self.next_page_id = 1;
    }

    fn next_page_id_raw(&mut self) -> NonZeroU32 {
        let id = NonZeroU32::new(self.next_page_id).unwrap_or_else(|| {
            self.next_page_id = 2;
            NonZeroU32::new(1).unwrap()
        });
        self.next_page_id += 1;
        id
    }

    pub fn page_counts(&self) -> (usize, usize, usize) {
        (self.alpha.len(), self.subpixel.len(), self.color.len())
    }

    fn create_page_texture(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        page_size: u32,
        label: &str,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: page_size,
                height: page_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        view: &wgpu::TextureView,
        label: &str,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    pub fn allocate_alpha(
        &mut self,
        size: PixelSize,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        linear_sampler: &wgpu::Sampler,
        nearest_sampler: &wgpu::Sampler,
        frame: u64,
    ) -> Option<PageAllocResult<AlphaMask>> {
        if !self.config.can_fit(size) {
            return None;
        }

        for page in &mut self.alpha {
            if let Some(allocation) = page.allocator.allocate(size) {
                page.pinned_this_frame = true;
                page.last_used_frame = frame;
                return Some(PageAllocResult {
                    page_id: page.id,
                    generation: page.generation,
                    allocation,
                });
            }
        }
        if self.alpha.len() >= self.config.max_pages_per_material {
            return None;
        }
        let id = PageId::new(self.next_page_id_raw());
        let (texture, view) = Self::create_page_texture(
            device,
            AlphaMask::TEXTURE_FORMAT,
            self.config.page_size,
            "Atlas Alpha Page",
        );
        let linear_bind_group = Self::create_bind_group(
            device,
            layout,
            linear_sampler,
            &view,
            "Atlas Alpha Page Linear Bind Group",
        );
        let nearest_bind_group = Self::create_bind_group(
            device,
            layout,
            nearest_sampler,
            &view,
            "Atlas Alpha Page Nearest Bind Group",
        );
        let allocator = ShelfAllocator::new(self.config.page_size, self.config.padding);
        self.alpha.push(AtlasPage {
            id,
            texture,
            linear_bind_group,
            nearest_bind_group,
            allocator,
            generation: 0,
            pinned_this_frame: true,
            last_used_frame: frame,
        });
        let page = self.alpha.last_mut().unwrap();
        let generation = page.generation;
        let allocation = page.allocator.allocate(size)?;
        Some(PageAllocResult {
            page_id: id,
            generation,
            allocation,
        })
    }

    pub fn allocate_subpixel(
        &mut self,
        size: PixelSize,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        linear_sampler: &wgpu::Sampler,
        nearest_sampler: &wgpu::Sampler,
        frame: u64,
    ) -> Option<PageAllocResult<SubpixelMask>> {
        if !self.config.can_fit(size) {
            return None;
        }

        for page in &mut self.subpixel {
            if let Some(allocation) = page.allocator.allocate(size) {
                page.pinned_this_frame = true;
                page.last_used_frame = frame;
                return Some(PageAllocResult {
                    page_id: page.id,
                    generation: page.generation,
                    allocation,
                });
            }
        }
        if self.subpixel.len() >= self.config.max_pages_per_material {
            return None;
        }
        let id = PageId::new(self.next_page_id_raw());
        let (texture, view) = Self::create_page_texture(
            device,
            SubpixelMask::TEXTURE_FORMAT,
            self.config.page_size,
            "Atlas Subpixel Page",
        );
        let linear_bind_group = Self::create_bind_group(
            device,
            layout,
            linear_sampler,
            &view,
            "Atlas Subpixel Page Linear Bind Group",
        );
        let nearest_bind_group = Self::create_bind_group(
            device,
            layout,
            nearest_sampler,
            &view,
            "Atlas Subpixel Page Nearest Bind Group",
        );
        let allocator = ShelfAllocator::new(self.config.page_size, self.config.padding);
        self.subpixel.push(AtlasPage {
            id,
            texture,
            linear_bind_group,
            nearest_bind_group,
            allocator,
            generation: 0,
            pinned_this_frame: true,
            last_used_frame: frame,
        });
        let page = self.subpixel.last_mut().unwrap();
        let generation = page.generation;
        let allocation = page.allocator.allocate(size)?;
        Some(PageAllocResult {
            page_id: id,
            generation,
            allocation,
        })
    }

    pub fn allocate_color(
        &mut self,
        size: PixelSize,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        linear_sampler: &wgpu::Sampler,
        nearest_sampler: &wgpu::Sampler,
        frame: u64,
    ) -> Option<PageAllocResult<ColorRgba>> {
        if !self.config.can_fit(size) {
            return None;
        }

        for page in &mut self.color {
            if let Some(allocation) = page.allocator.allocate(size) {
                page.pinned_this_frame = true;
                page.last_used_frame = frame;
                return Some(PageAllocResult {
                    page_id: page.id,
                    generation: page.generation,
                    allocation,
                });
            }
        }
        if self.color.len() >= self.config.max_pages_per_material {
            return None;
        }
        let id = PageId::new(self.next_page_id_raw());
        let (texture, view) = Self::create_page_texture(
            device,
            ColorRgba::TEXTURE_FORMAT,
            self.config.page_size,
            "Atlas Color Page",
        );
        let linear_bind_group = Self::create_bind_group(
            device,
            layout,
            linear_sampler,
            &view,
            "Atlas Color Page Linear Bind Group",
        );
        let nearest_bind_group = Self::create_bind_group(
            device,
            layout,
            nearest_sampler,
            &view,
            "Atlas Color Page Nearest Bind Group",
        );
        let allocator = ShelfAllocator::new(self.config.page_size, self.config.padding);
        self.color.push(AtlasPage {
            id,
            texture,
            linear_bind_group,
            nearest_bind_group,
            allocator,
            generation: 0,
            pinned_this_frame: true,
            last_used_frame: frame,
        });
        let page = self.color.last_mut().unwrap();
        let generation = page.generation;
        let allocation = page.allocator.allocate(size)?;
        Some(PageAllocResult {
            page_id: id,
            generation,
            allocation,
        })
    }

    pub fn alpha_page(&self, id: PageId<AlphaMask>) -> Option<&AtlasPage<AlphaMask>> {
        self.alpha.iter().find(|p| p.id == id)
    }

    pub fn subpixel_page(&self, id: PageId<SubpixelMask>) -> Option<&AtlasPage<SubpixelMask>> {
        self.subpixel.iter().find(|p| p.id == id)
    }

    pub fn color_page(&self, id: PageId<ColorRgba>) -> Option<&AtlasPage<ColorRgba>> {
        self.color.iter().find(|p| p.id == id)
    }

    pub fn begin_frame(&mut self) {
        for page in &mut self.alpha {
            page.pinned_this_frame = false;
        }
        for page in &mut self.subpixel {
            page.pinned_this_frame = false;
        }
        for page in &mut self.color {
            page.pinned_this_frame = false;
        }
    }

    pub fn pin_alpha(&mut self, id: PageId<AlphaMask>, frame: u64) {
        if let Some(page) = self.alpha.iter_mut().find(|p| p.id == id) {
            page.pinned_this_frame = true;
            page.last_used_frame = frame;
        }
    }

    pub fn pin_subpixel(&mut self, id: PageId<SubpixelMask>, frame: u64) {
        if let Some(page) = self.subpixel.iter_mut().find(|p| p.id == id) {
            page.pinned_this_frame = true;
            page.last_used_frame = frame;
        }
    }

    pub fn pin_color(&mut self, id: PageId<ColorRgba>, frame: u64) {
        if let Some(page) = self.color.iter_mut().find(|p| p.id == id) {
            page.pinned_this_frame = true;
            page.last_used_frame = frame;
        }
    }

    pub fn lru_unpinned_alpha(&self) -> Option<usize> {
        self.alpha
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.pinned_this_frame)
            .min_by_key(|(_, p)| p.last_used_frame)
            .map(|(i, _)| i)
    }

    pub fn lru_unpinned_subpixel(&self) -> Option<usize> {
        self.subpixel
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.pinned_this_frame)
            .min_by_key(|(_, p)| p.last_used_frame)
            .map(|(i, _)| i)
    }

    pub fn lru_unpinned_color(&self) -> Option<usize> {
        self.color
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.pinned_this_frame)
            .min_by_key(|(_, p)| p.last_used_frame)
            .map(|(i, _)| i)
    }

    pub fn reset_alpha_page(
        &mut self,
        index: usize,
        frame: u64,
    ) -> Option<(PageId<AlphaMask>, u32)> {
        let page = self.alpha.get_mut(index)?;
        page.generation = page.generation.wrapping_add(1);
        page.allocator = ShelfAllocator::new(self.config.page_size, self.config.padding);
        page.pinned_this_frame = true;
        page.last_used_frame = frame;
        Some((page.id, page.generation))
    }

    pub fn reset_subpixel_page(
        &mut self,
        index: usize,
        frame: u64,
    ) -> Option<(PageId<SubpixelMask>, u32)> {
        let page = self.subpixel.get_mut(index)?;
        page.generation = page.generation.wrapping_add(1);
        page.allocator = ShelfAllocator::new(self.config.page_size, self.config.padding);
        page.pinned_this_frame = true;
        page.last_used_frame = frame;
        Some((page.id, page.generation))
    }

    pub fn reset_color_page(
        &mut self,
        index: usize,
        frame: u64,
    ) -> Option<(PageId<ColorRgba>, u32)> {
        let page = self.color.get_mut(index)?;
        page.generation = page.generation.wrapping_add(1);
        page.allocator = ShelfAllocator::new(self.config.page_size, self.config.padding);
        page.pinned_this_frame = true;
        page.last_used_frame = frame;
        Some((page.id, page.generation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_counts_start_at_zero() {
        let pages = GlyphAtlasPages::new(GlyphAtlasConfig::default());
        assert_eq!(pages.page_counts(), (0, 0, 0));
    }

    #[test]
    fn next_page_id_increments() {
        let mut pages = GlyphAtlasPages::new(GlyphAtlasConfig::default());
        let id1 = pages.next_page_id_raw();
        let id2 = pages.next_page_id_raw();
        assert!(id2.get() > id1.get());
    }

    #[test]
    fn clear_resets_page_id_counter() {
        let mut pages = GlyphAtlasPages::new(GlyphAtlasConfig::default());
        let _ = pages.next_page_id_raw();
        let _ = pages.next_page_id_raw();
        pages.clear();
        let id_after = pages.next_page_id_raw();
        assert_eq!(id_after.get(), 1);
    }
}
