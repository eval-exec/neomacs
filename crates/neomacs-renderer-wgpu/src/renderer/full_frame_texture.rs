//! The one way to allocate a GPU texture whose size follows a frame window's
//! surface.
//!
//! [`GpuBudget`] can only be as honest as the allocations it is told about,
//! and until now nothing connected the two. The census key
//! ([`UnpooledTexture`]) was exhaustive, but it was a value someone had to
//! remember to reach for: a full-frame texture was a bare
//! `device.create_texture` call and a `u64` computed a second time, somewhere
//! else, by whoever wrote the per-frame census. Two things could therefore go
//! wrong silently — a new full-frame texture that named no category at all,
//! and a census entry describing a size the texture no longer had.
//!
//! `FullFrameTexture` closes both. Its constructor takes an
//! [`UnpooledTexture`] by value, so an allocation that declares no category
//! does not compile; and it computes its own budget cost from the very
//! descriptor it allocated with, so no caller is in a position to report a
//! different number. The census then takes the texture rather than a
//! `(kind, bytes)` pair — see `WgpuRenderer::record_full_frame_texture` —
//! which leaves nothing for a call site to get wrong.
//!
//! Textures whose size follows *content* rather than the window — glyph-atlas
//! pages, decoded images, video frames, WebView surfaces, imported DMA-BUFs —
//! are deliberately not this type. They are bounded by
//! [`media_budget`](crate::media_budget) and by the atlas's own page census,
//! and forcing them through a full-frame ceiling would constrain the wrong
//! thing. `texture_discipline_test.rs` is what keeps that distinction a
//! stated decision per file rather than an accident.
//!
//! [`GpuBudget`]: super::gpu_budget::GpuBudget

use super::gpu_budget::UnpooledTexture;
use super::snapshot_pool::{SnapshotSize, texture_bytes};

/// A GPU texture sized to a frame window's surface, carrying the budget
/// category it is counted under and what it costs.
///
/// Not a pool lease: these are the full-frame textures whose owning structs
/// outlive a single frame's draw and therefore cannot hold one. What they get
/// instead is this — an allocation that cannot exist without having named its
/// census category.
pub struct FullFrameTexture {
    /// Held, never read, exactly as a pool slot holds its own: this is the
    /// handle the budget was charged for, and the allocation's lifetime should
    /// be a fact of this struct rather than a consequence of wgpu's internal
    /// refcounting behind the view.
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    kind: UnpooledTexture,
    size: SnapshotSize,
    budget_bytes: u64,
}

impl FullFrameTexture {
    /// Allocate a full-frame texture under `kind`.
    ///
    /// `size` is a [`SnapshotSize`] rather than two `u32`s because wgpu
    /// rejects a zero extent, and a caller that has not yet decided what to do
    /// about a zero-sized window should be stopped before it reaches the
    /// device rather than after.
    pub fn allocate(
        device: &wgpu::Device,
        kind: UnpooledTexture,
        label: &'static str,
        size: SnapshotSize,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size.width(),
                height: size.height(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            _texture: texture,
            view,
            kind,
            size,
            budget_bytes: texture_bytes(size, format),
        }
    }

    /// Which census entry this texture is counted under.
    pub fn kind(&self) -> UnpooledTexture {
        self.kind
    }

    /// The size it was allocated at, which is the size it still is: nothing
    /// resizes one of these, a new one replaces it.
    pub fn size(&self) -> SnapshotSize {
        self.size
    }

    /// What it costs the GPU budget, measured from the descriptor it was
    /// allocated with.
    pub fn budget_bytes(&self) -> u64 {
        self.budget_bytes
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
}
