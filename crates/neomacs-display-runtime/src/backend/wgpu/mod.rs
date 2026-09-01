//! Winit + wgpu GPU-accelerated display backend.

mod animation;
mod events;
pub mod toolbar_icons;

pub mod media_budget {
    //! Moved beside the renderer caches (real byte counts live there);
    //! re-exported here so existing `backend::wgpu::media_budget` paths work.
    pub use neomacs_renderer_wgpu::media_budget::*;
}

#[cfg(feature = "video")]
pub use neomacs_renderer_wgpu::{CachedVideo, VideoCache, VideoState};

pub use neomacs_renderer_wgpu::{
    BufferFormat, CachedImage, ExternalBuffer, GlyphKey, GlyphVertex, ImageCache, ImageState,
    PlatformBuffer, SharedMemoryBuffer, WgpuGlyphAtlas, WgpuRenderer,
};

#[cfg(target_os = "linux")]
pub use neomacs_renderer_wgpu::DmaBufBuffer;

pub use animation::{AnimatedProperty, Animation, AnimationEngine, AnimationTarget, Easing};
pub use events::{
    EventKind, NEOMACS_CTRL_MASK, NEOMACS_EVENT_BUTTON_PRESS, NEOMACS_EVENT_BUTTON_RELEASE,
    NEOMACS_EVENT_CLOSE, NEOMACS_EVENT_FILE_DROP, NEOMACS_EVENT_FOCUS_IN, NEOMACS_EVENT_FOCUS_OUT,
    NEOMACS_EVENT_IMAGE_DIMENSIONS_READY, NEOMACS_EVENT_KEY_PRESS, NEOMACS_EVENT_KEY_RELEASE,
    NEOMACS_EVENT_MENU_BAR_CLICK, NEOMACS_EVENT_MENU_SELECTION, NEOMACS_EVENT_MOUSE_MOVE,
    NEOMACS_EVENT_RESIZE, NEOMACS_EVENT_SCROLL, NEOMACS_EVENT_TERMINAL_EXITED,
    NEOMACS_EVENT_TERMINAL_TITLE_CHANGED, NEOMACS_EVENT_TOOL_BAR_CLICK, NEOMACS_META_MASK,
    NEOMACS_SHIFT_MASK, NEOMACS_SUPER_MASK, NeomacsInputEvent,
};
#[cfg(all(feature = "webview", target_os = "linux"))]
pub use neomacs_renderer_wgpu::{CachedWebView, WgpuWebViewCache};

// DRM device discovery for GPU device path mapping
#[cfg(target_os = "linux")]
mod drm_device;

#[cfg(target_os = "linux")]
pub use drm_device::{
    DrmDeviceInfo, find_drm_render_nodes, find_render_node_for_adapter,
    get_render_node_from_adapter_info,
};
