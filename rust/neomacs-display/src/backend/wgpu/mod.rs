//! Winit + wgpu GPU-accelerated display backend.

#[cfg(feature = "winit-backend")]
mod vertex;
#[cfg(feature = "winit-backend")]
mod renderer;
#[cfg(feature = "winit-backend")]
mod backend;

#[cfg(feature = "winit-backend")]
pub use renderer::WgpuRenderer;
#[cfg(feature = "winit-backend")]
pub use backend::WinitBackend;
