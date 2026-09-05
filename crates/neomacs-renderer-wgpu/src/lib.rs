//! WGPU renderer primitives shared by display backends.

// Renderer entry points and GPU-pipeline builders take many positional
// parameters (geometry, colors, atlas/pipeline handles); folding them into
// structs is a separate refactor, so this bulk category is allowed crate-wide.
#![allow(clippy::too_many_arguments)]

pub mod device_request;
pub mod external_buffer;
pub mod frame_post;
pub mod glyph_atlas;
#[cfg(feature = "video")]
mod gpu_frame_timing;
pub mod image_cache;
mod image_sequence;
pub mod media_budget;
pub mod overlay_state;
pub mod renderer;
pub mod shader_surface;
pub mod shader_surface_cache;
mod svg;
pub use svg::SvgResourceContext;
pub mod vertex;
pub mod xbm;
pub mod xpm;

#[cfg(feature = "video")]
pub mod video_cache;

#[cfg(all(feature = "webview", target_os = "linux"))]
pub mod webview_cache;

#[cfg(all(feature = "webview", target_os = "linux"))]
mod submission_retirement;

#[cfg(all(
    any(feature = "video-dmabuf", feature = "webview"),
    target_os = "linux"
))]
pub mod vulkan_dmabuf;

pub use device_request::request_renderer_device;
#[cfg(target_os = "linux")]
pub use external_buffer::DmaBufBuffer;
pub use external_buffer::{BufferFormat, ExternalBuffer, PlatformBuffer, SharedMemoryBuffer};
pub use glyph_atlas::{
    ComposedGlyphKey, GlyphAtlasHandle, GlyphKey, GlyphPixelKind, RasterizeResult, WgpuGlyphAtlas,
    allocator, pages, types,
};
pub use image_cache::{CachedImage, ImageCache, ImageCacheEvent, ImageMetadata, ImageState};
pub use overlay_state::{MenuPanel, PopupMenuState, TooltipState};
pub use renderer::{
    BudgetExceeded, CompositionRing, FrameRowDamage, GpuBudget, PaneBlit, PaneSource,
    RendererFrameEffects, RowDamageInfo, RowReuseStats, SnapshotId, SnapshotLease, SnapshotPool,
    SnapshotResources, SnapshotSize, UnpooledTexture, WgpuRenderer, WindowRowDamage, texture_bytes,
};
pub use shader_surface::{
    SURFACE_USER_UNIFORM_SLOTS, ShaderValidationError, SurfaceContract, SurfaceUniformInit,
    compose_surface_wgsl, validate_surface_wgsl,
};
pub use shader_surface_cache::{MAX_SURFACE_SIZE, ShaderSurfaceCache};
pub use vertex::{GlyphVertex, RectVertex, RoundedRectVertex, TextureVertex, Uniforms};
#[cfg(feature = "video")]
pub use video_cache::{CachedVideo, VideoCache, VideoRecoveryManifest, VideoState};
#[cfg(all(feature = "webview", target_os = "linux"))]
pub use webview_cache::{CachedWebView, WgpuWebViewCache};

/// Whether the WGPU backend has a concrete rendering path for a graphical
/// face feature.
///
/// Keep this match exhaustive: extending the shared capability domain should
/// fail this crate's build until the renderer makes an explicit decision.
pub const fn supports_graphical_face_attribute(
    attribute: neomacs_display_protocol::GraphicalFaceAttribute,
) -> bool {
    use neomacs_display_protocol::{GraphicalFaceAttribute, UnderlineStyle};

    match attribute {
        GraphicalFaceAttribute::Foreground
        | GraphicalFaceAttribute::Background
        | GraphicalFaceAttribute::DistantForeground
        | GraphicalFaceAttribute::Stipple
        | GraphicalFaceAttribute::Underline(
            UnderlineStyle::None
            | UnderlineStyle::Line
            | UnderlineStyle::Double
            | UnderlineStyle::Wave
            | UnderlineStyle::Dotted
            | UnderlineStyle::Dashed,
        )
        | GraphicalFaceAttribute::Overline
        | GraphicalFaceAttribute::StrikeThrough
        | GraphicalFaceAttribute::Box
        | GraphicalFaceAttribute::InverseVideo
        | GraphicalFaceAttribute::Extend => true,
    }
}

/// Re-exported effect configuration module for renderer internals and callers.
pub mod effect_config {
    pub use neomacs_display_protocol::effect_config::*;
}

/// Read GPU power preference from `NEOMACS_GPU`.
pub fn gpu_power_preference() -> wgpu::PowerPreference {
    match std::env::var("NEOMACS_GPU").as_deref() {
        Ok("low") | Ok("integrated") => wgpu::PowerPreference::LowPower,
        Ok("high") | Ok("discrete") => wgpu::PowerPreference::HighPerformance,
        _ => wgpu::PowerPreference::HighPerformance,
    }
}
