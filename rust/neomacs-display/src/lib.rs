//! Neomacs Display Engine
//!
//! A GPU-accelerated display engine for Neomacs using WPE WebKit and wgpu.
//!
//! # Architecture
//!
//! ```text
//! Emacs Core (C) ──► FFI ──► Scene Graph ──► wgpu ──► GPU
//! ```

#![allow(unused)] // TODO: Remove once implementation is complete

pub mod core;
pub mod backend;
pub mod text;
pub mod ffi;
pub mod thread_comm;
pub mod layout;

#[cfg(feature = "winit-backend")]
pub mod render_thread;

#[cfg(feature = "neo-term")]
pub mod terminal;

pub use crate::core::*;
pub use crate::backend::DisplayBackend;
pub use crate::text::TextEngine;

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Read GPU power preference from `NEOMACS_GPU` environment variable.
///
/// - `"low"` or `"integrated"` → `LowPower` (prefer integrated GPU, e.g. Intel)
/// - `"high"` or `"discrete"` → `HighPerformance` (prefer discrete GPU, e.g. NVIDIA)
/// - unset or anything else → `HighPerformance` (default)
#[cfg(feature = "winit-backend")]
pub fn gpu_power_preference() -> wgpu::PowerPreference {
    match std::env::var("NEOMACS_GPU").as_deref() {
        Ok("low") | Ok("integrated") => {
            log::info!("NEOMACS_GPU={}: using LowPower (integrated GPU)", std::env::var("NEOMACS_GPU").unwrap());
            wgpu::PowerPreference::LowPower
        }
        Ok("high") | Ok("discrete") => {
            log::info!("NEOMACS_GPU=high: using HighPerformance (discrete GPU)");
            wgpu::PowerPreference::HighPerformance
        }
        Ok(val) => {
            log::warn!("NEOMACS_GPU={}: unrecognized value, defaulting to HighPerformance", val);
            wgpu::PowerPreference::HighPerformance
        }
        Err(_) => wgpu::PowerPreference::HighPerformance,
    }
}

/// Initialize the display engine
pub fn init() -> Result<(), DisplayError> {
    env_logger::init();
    log::info!("Neomacs display engine v{} initializing (wgpu backend)", VERSION);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
