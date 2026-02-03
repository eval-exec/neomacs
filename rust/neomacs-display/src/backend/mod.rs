//! Backend trait and module exports.

use crate::core::error::DisplayResult;
use crate::core::scene::Scene;

#[cfg(feature = "gtk4-backend")]
pub mod gtk4;
pub mod tty;

#[cfg(feature = "winit-backend")]
pub mod wgpu;

#[cfg(feature = "wpe-webkit")]
pub mod wpe;

// Legacy webkit module - being replaced by wpe
#[cfg(feature = "wpe-webkit")]
pub mod webkit;

// Stub WebKitCache when webkit is disabled
#[cfg(all(not(feature = "wpe-webkit"), feature = "gtk4-backend"))]
pub mod webkit {
    use gtk4::gdk;

    /// Stub WebKitCache for when webkit is disabled
    pub struct WebKitCache;

    impl WebKitCache {
        pub fn new() -> Self { Self }
        pub fn get(&self, _id: u32) -> Option<&StubView> { None }
    }

    impl Default for WebKitCache {
        fn default() -> Self { Self::new() }
    }

    /// Stub view
    pub struct StubView;

    impl StubView {
        pub fn texture(&self) -> Option<&gdk::Texture> { None }
    }
}

// Stub WebKitCache when both wpe-webkit and gtk4-backend are disabled
#[cfg(all(not(feature = "wpe-webkit"), not(feature = "gtk4-backend")))]
pub mod webkit {
    /// Stub WebKitCache for when webkit and gtk4 are disabled
    pub struct WebKitCache;

    impl WebKitCache {
        pub fn new() -> Self { Self }
        pub fn get(&self, _id: u32) -> Option<&StubView> { None }
    }

    impl Default for WebKitCache {
        fn default() -> Self { Self::new() }
    }

    /// Stub view
    pub struct StubView;
}

/// Display backend trait
///
/// Implementations provide platform-specific rendering.
/// Note: GTK4 backend is not Send+Sync because GTK is single-threaded.
pub trait DisplayBackend {
    /// Initialize the backend
    fn init(&mut self) -> DisplayResult<()>;

    /// Shutdown the backend
    fn shutdown(&mut self);

    /// Render a scene to the display
    fn render(&mut self, scene: &Scene) -> DisplayResult<()>;

    /// Present the rendered frame
    fn present(&mut self) -> DisplayResult<()>;

    /// Get the backend name
    fn name(&self) -> &'static str;

    /// Check if the backend is initialized
    fn is_initialized(&self) -> bool;

    /// Handle resize
    fn resize(&mut self, width: u32, height: u32);

    /// Set VSync enabled
    fn set_vsync(&mut self, enabled: bool);
}

/// Backend type selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum BackendType {
    /// GTK4/GSK GPU-accelerated backend
    #[cfg(feature = "gtk4-backend")]
    Gtk4 = 0,

    /// Terminal/TTY backend
    Tty = 1,

    /// Winit/wgpu GPU-accelerated backend
    #[cfg(feature = "winit-backend")]
    Wgpu = 2,
}

impl Default for BackendType {
    fn default() -> Self {
        #[cfg(feature = "winit-backend")]
        return Self::Wgpu;

        #[cfg(all(not(feature = "winit-backend"), feature = "gtk4-backend"))]
        return Self::Gtk4;

        #[cfg(all(not(feature = "winit-backend"), not(feature = "gtk4-backend")))]
        return Self::Tty;
    }
}
