//! Grouped GPU resources owned by `WgpuRenderer`: render pipelines, the
//! stencil clip targets, texture/media caches, and per-frame vertex arenas.

use super::super::image_cache::ImageCache;
use super::super::shader_surface_cache::ShaderSurfaceCache;
#[cfg(feature = "video")]
use super::super::video_cache::VideoCache;
#[cfg(all(feature = "webview", target_os = "linux"))]
use super::super::webview_cache::WgpuWebViewCache;
use super::dynamic_buffer::FrameVertexArena;
use super::full_frame_texture::FullFrameTexture;
use super::gpu_budget::UnpooledTexture;
use super::snapshot_pool::SnapshotSize;
use crate::vertex::{GlyphVertex, RectVertex, RoundedRectVertex, SubpixelGlyphVertex};

/// All render pipelines. The `stencil_*` variants are identical to their base
/// counterparts except for stencil state; they draw only where the stencil
/// buffer was written (child frame rounded-corner clipping).
pub(crate) struct Pipelines {
    pub(crate) rect: wgpu::RenderPipeline,
    pub(crate) rounded_rect: wgpu::RenderPipeline,
    pub(crate) corner_mask: wgpu::RenderPipeline,
    pub(crate) glyph: wgpu::RenderPipeline,
    pub(crate) subpixel_glyph: wgpu::RenderPipeline,
    pub(crate) image: wgpu::RenderPipeline,
    #[cfg(feature = "video")]
    pub(crate) bi_planar_video: wgpu::RenderPipeline,
    #[cfg(feature = "video")]
    pub(crate) bi_planar_video_copy: wgpu::RenderPipeline,
    pub(crate) opaque_image: wgpu::RenderPipeline,
    pub(crate) stencil_rect: wgpu::RenderPipeline,
    pub(crate) stencil_rounded_rect: wgpu::RenderPipeline,
    pub(crate) stencil_glyph: wgpu::RenderPipeline,
    pub(crate) stencil_subpixel_glyph: wgpu::RenderPipeline,
    pub(crate) stencil_image: wgpu::RenderPipeline,
    #[cfg(feature = "video")]
    pub(crate) stencil_bi_planar_video: wgpu::RenderPipeline,
    pub(crate) stencil_opaque_image: wgpu::RenderPipeline,
    pub(crate) stencil_write: wgpu::RenderPipeline,
}

/// The format of the stencil clip target.
///
/// A stencil texel is one byte, not the four a colour attachment costs, so a
/// full-frame target is about 3.7 MB at 2560x1440 rather than 15. Small beside
/// a glyph atlas, and still real: a ceiling is only worth consulting if it can
/// say what is under it, and an allocation nobody counts is headroom the pool
/// believes it can hand out twice.
pub(crate) const STENCIL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Stencil8;

/// The size a stencil clip target is actually allocated at for a window of
/// `width` by `height` device pixels.
///
/// wgpu rejects a zero extent, so a degenerate window still gets one texel.
/// Named separately from the constructor only so a test with no GPU can ask
/// what the clamp does; the constructor is its sole production caller, which
/// keeps this one description of the size rather than a second one.
pub(crate) fn stencil_clip_size(width: u32, height: u32) -> SnapshotSize {
    SnapshotSize::new(width.max(1), height.max(1))
        .expect("a dimension clamped to at least one is non-zero")
}

/// Stencil texture/view used to clip child frames to rounded corners.
/// Recreated on resize.
pub(crate) struct StencilTargets {
    /// A [`FullFrameTexture`] rather than a raw handle: this is the one
    /// renderer-owned allocation that tracks the window size, so it has to
    /// carry its own census category and cost. The arithmetic that charges it
    /// used to live in a separate function beside the descriptor, which is two
    /// descriptions of one texture and therefore two things that can drift.
    pub(crate) texture: FullFrameTexture,
}

impl StencilTargets {
    /// A clip target covering `width` by `height` device pixels.
    ///
    /// The renderer reaches this only through
    /// `WgpuRenderer::install_stencil_targets`, which reports the GPU budget
    /// in the same statement that installs the result — so there is one place
    /// a stencil target starts being used and it is the place that pays for it.
    ///
    /// Clamps rather than refusing a zero dimension: `resize` already returns
    /// early on one, and the clamp is what lets [`Self::placeholder`] exist at
    /// all.
    pub(crate) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let size = stencil_clip_size(width, height);
        Self {
            texture: FullFrameTexture::allocate(
                device,
                UnpooledTexture::StencilClip,
                "Stencil Texture",
                size,
                STENCIL_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ),
        }
    }

    /// A one-texel target for the moment before the renderer exists.
    ///
    /// The renderer is built by a struct literal, and a literal cannot call
    /// the method that reports the budget. Starting from a target that costs a
    /// byte, then installing the real one, keeps `install_stencil_targets` the
    /// single allocation site rather than the second of two.
    pub(crate) fn placeholder(device: &wgpu::Device) -> Self {
        Self::new(device, 1, 1)
    }
}

/// Texture/media caches.
pub(crate) struct RenderCaches {
    pub(crate) image: ImageCache,
    #[cfg(feature = "video")]
    pub(crate) video: VideoCache,
    #[cfg(all(feature = "webview", target_os = "linux"))]
    pub(crate) webview: WgpuWebViewCache,
    pub(crate) surface: ShaderSurfaceCache,
}

/// Per-frame reusable vertex upload arenas.
///
/// `begin_frame` resets every arena and is called once per frame cycle from
/// `render_frame_glyphs`, which display-runtime always runs first in a frame.
/// Later entry points in the same cycle (overlays, child frames, transitions)
/// bump-allocate after it; each submits its encoder before returning, and the
/// queue is in-order, so a reset can never clobber an unconsumed region.
pub(crate) struct VertexArenas {
    pub(crate) glyph: FrameVertexArena<GlyphVertex>,
    pub(crate) subpixel: FrameVertexArena<SubpixelGlyphVertex>,
    /// Textured quads drawn through the image pipelines (inline/floating
    /// images, videos, WebViews, blits, transition quads).
    pub(crate) image: FrameVertexArena<GlyphVertex>,
    /// Solid-color quads (backgrounds, cursors, decorations, effects).
    pub(crate) rect: FrameVertexArena<RectVertex>,
    /// SDF rounded-rect quads (box fills/borders, scroll thumbs, masks).
    pub(crate) rounded: FrameVertexArena<RoundedRectVertex>,
    /// `buffers_created` total at the previous per-frame snapshot.
    created_snapshot: u64,
}

impl VertexArenas {
    pub(crate) fn new() -> Self {
        Self {
            glyph: FrameVertexArena::new("Glyph Vertex Arena"),
            subpixel: FrameVertexArena::new("Subpixel Glyph Vertex Arena"),
            image: FrameVertexArena::new("Image Vertex Arena"),
            rect: FrameVertexArena::new("Rect Vertex Arena"),
            rounded: FrameVertexArena::new("Rounded Rect Vertex Arena"),
            created_snapshot: 0,
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.glyph.begin_frame();
        self.subpixel.begin_frame();
        self.image.begin_frame();
        self.rect.begin_frame();
        self.rounded.begin_frame();
    }

    fn buffers_created_total(&self) -> u64 {
        self.glyph.buffers_created()
            + self.subpixel.buffers_created()
            + self.image.buffers_created()
            + self.rect.buffers_created()
            + self.rounded.buffers_created()
    }

    /// GPU buffer allocations since the previous snapshot (i.e. this frame).
    /// Zero in steady state once the arenas reach their high-water capacity.
    pub(crate) fn buffers_created_since_snapshot(&mut self) -> u64 {
        let total = self.buffers_created_total();
        let delta = total - self.created_snapshot;
        self.created_snapshot = total;
        delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Arena construction and the per-frame counter bookkeeping are
    // device-free; actual `upload` (buffer creation + write_buffer) needs a
    // live wgpu device and stays untested on this headless box.
    #[test]
    fn fresh_arenas_report_zero_created_buffers_per_frame() {
        let mut arenas = VertexArenas::new();
        assert_eq!(arenas.buffers_created_since_snapshot(), 0);
        arenas.begin_frame();
        arenas.begin_frame();
        assert_eq!(arenas.buffers_created_since_snapshot(), 0);
    }
}
