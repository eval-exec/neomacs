//! Render thread implementation.
//!
//! Owns winit event loop, wgpu, GLib/WebKit. Runs at native VSync.

mod app_handler;
mod asset_commands;
mod bootstrap;
pub(crate) mod child_frames;
mod command_processing;
mod cursor;
mod cursor_runtime;
mod device_loss;
mod frame_compositor;
mod frame_ingest;
mod frame_preparation;
mod frame_sched;
mod frame_state;
pub(crate) mod frame_stats;
pub(crate) mod frame_windows;
mod gpu_startup;
mod input;
mod lifecycle;
mod media;
#[cfg(test)]
#[path = "tests/modifier_policy_cooking_test.rs"]
mod modifier_policy_cooking_test;
mod modifier_sides;
mod pointer_events;
pub(in crate::render_thread) mod render_pass;
mod render_quality;
mod scroll_input;
mod startup;
mod state;
mod surface_readback;
mod terminal_commands;
#[cfg(feature = "neo-term")]
mod terminal_expansion;
#[cfg(test)]
#[path = "tests/render_thread_test.rs"]
mod tests;
#[cfg(test)]
#[path = "tests/texture_discipline_test.rs"]
mod texture_discipline_test;

mod geometry_hints;
mod surface_resize;
mod thread_handle;
#[cfg(test)]
#[path = "tests/time_discipline_test.rs"]
mod time_discipline_test;
mod toolbar;
mod transitions;
mod ui_commands;
mod window_commands;
mod window_events;

#[cfg(feature = "neo-term")]
pub use bootstrap::run_render_loop_current_thread_with_terminals;
pub use bootstrap::{build_render_event_loop, run_render_loop, run_render_loop_current_thread};
pub(crate) use lifecycle::PopupCommit;
pub use startup::{
    InitialWindowLifetime, InitialWindowReceiver, InitialWindowReply, InitialWindowSize,
    RenderLoopError, RenderLoopExit,
};
use state::{FpsCounter, ImeCursorArea, RenderApp};
pub use state::{
    ImageDecodeTerminal, ImageRenderState, ImageTerminalProbe, ImageTerminalPublication,
    MonitorInfo, SharedImageRenderState, SharedMonitorInfo,
};
pub use thread_handle::RenderThread;

/// Header-only image geometry, for placing an image whose pixels have not been
/// decoded yet (see `neomacs_renderer_wgpu::image_probe`).
pub use neomacs_renderer_wgpu::image_probe::{ImageProbeSource, probe_image_layout};

/// Rows of an image whose decode is still running, as published by
/// `ImageDecodeTerminal::Band`.
pub use neomacs_renderer_wgpu::RowRange;

use winit::event_loop::EventLoopProxy;

pub type RenderEventLoopProxy = EventLoopProxy;
pub type RenderEventLoop = winit::event_loop::EventLoop;

// All GPU caches (image, video, webkit) are managed by WgpuRenderer
