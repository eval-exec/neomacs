//! Neomacs Display Runtime
//!
//! A GPU-accelerated display engine for Neomacs using WPE WebKit and wgpu.
//!
//! # Architecture
//!
//! ```text
//! Editor Runtime (Rust) ──► Scene Graph ──► wgpu ──► GPU
//! ```

// Backend/window entry points and GPU-plumbing helpers take many positional
// parameters (geometry, handles, callbacks); folding them into structs is a
// separate refactor, so this bulk category is allowed crate-wide.
#![allow(clippy::too_many_arguments)]

pub mod backend;
pub(crate) mod clipboard;
pub mod core;
pub mod display_scale;
pub mod macos_bundle_runtime;
pub mod thread_comm;
pub mod tty_input;
#[cfg(target_os = "linux")]
mod wayland_toplevel_icon;
mod window_icon;
mod window_identity;

pub mod render_thread;

#[cfg(feature = "neo-term")]
pub mod terminal;

/// Layout-facing font matching helpers (kept under the legacy module path).
pub mod font_match {
    pub use neomacs_layout_engine::font::font_match::*;
}

/// Rust layout engine API (kept under the legacy module path).
pub mod layout {
    pub use neomacs_layout_engine::*;
}

pub use crate::core::*;
pub use neomacs_renderer_wgpu::supports_graphical_face_attribute;

/// Shader-surface composition/validation (re-exported so the frontend can
/// naga-validate user WGSL on the Lisp thread without a renderer dependency).
pub mod shader_surface {
    pub use neomacs_renderer_wgpu::shader_surface::*;
}

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// NeoVM core backend selected at compile time.
pub const CORE_BACKEND: &str = "rust";

pub use render_thread::frame_stats::{
    DEMAND_REASON_NAMES, FRAME_TIME_BUCKET_UPPER_US, FrameSchedSnapshot, WindowFrameSnapshot,
};

#[cfg(test)]
mod frame_metrics_pub_test;

/// Read the current process-global frame-scheduling counters.
///
/// Safe to call from any thread — the counters are relaxed atomics — so the
/// diagnostics server can sample frame timing without touching the render
/// thread's state.
pub fn frame_metrics_snapshot() -> FrameSchedSnapshot {
    render_thread::frame_stats::snapshot()
}

/// Read the per-native-window demand attribution (currently-active demand
/// reasons and per-reason planned-frame counts), sorted by window id.
///
/// Safe to call from any thread: the render thread publishes into the shared
/// map only on event-loop wakes it was already performing, so sampling this
/// never wakes or blocks the render loop at idle.
pub fn window_frame_metrics_snapshot() -> Vec<WindowFrameSnapshot> {
    render_thread::frame_stats::window_snapshots()
}

/// Read GPU power preference from `NEOMACS_GPU` environment variable.
///
/// - `"low"` or `"integrated"` → `LowPower` (prefer integrated GPU, e.g. Intel)
/// - `"high"` or `"discrete"` → `HighPerformance` (prefer discrete GPU, e.g. NVIDIA)
/// - unset or anything else → `HighPerformance` (default)
pub fn gpu_power_preference() -> wgpu::PowerPreference {
    match std::env::var("NEOMACS_GPU").as_deref() {
        Ok("low") | Ok("integrated") => {
            tracing::info!(
                "NEOMACS_GPU={}: using LowPower (integrated GPU)",
                std::env::var("NEOMACS_GPU").unwrap()
            );
            wgpu::PowerPreference::LowPower
        }
        Ok("high") | Ok("discrete") => {
            tracing::info!("NEOMACS_GPU=high: using HighPerformance (discrete GPU)");
            wgpu::PowerPreference::HighPerformance
        }
        Ok(val) => {
            tracing::warn!(
                "NEOMACS_GPU={}: unrecognized value, defaulting to HighPerformance",
                val
            );
            wgpu::PowerPreference::HighPerformance
        }
        Err(_) => wgpu::PowerPreference::HighPerformance,
    }
}

pub(crate) fn wgpu_instance_descriptor_with_display(
    display: winit::event_loop::OwnedDisplayHandle,
) -> wgpu::InstanceDescriptor {
    wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(display))
}

/// Initialize the display engine.
///
/// Logging is initialized separately by the binary entry point via
/// `neovm_core::logging::init()` and is assumed to already be set up
/// when this function runs.
pub fn init() -> Result<(), DisplayError> {
    tracing::info!(
        "Neomacs display engine v{} initializing (wgpu backend)",
        VERSION
    );
    Ok(())
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
