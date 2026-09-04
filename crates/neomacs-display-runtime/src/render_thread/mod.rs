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
mod frame_sched;
mod frame_state;
pub(crate) mod frame_stats;
pub(crate) mod frame_windows;
mod input;
mod lifecycle;
mod media;
mod pointer_events;
mod render_pass;
mod render_quality;
mod state;
mod surface_readback;
mod terminal_commands;
#[cfg(feature = "neo-term")]
mod terminal_expansion;
#[cfg(test)]
mod tests;

mod thread_handle;
#[cfg(test)]
#[path = "time_discipline_test.rs"]
mod time_discipline_test;
mod transitions;
mod ui_commands;
mod window_commands;
mod window_events;
mod x11_hints;

#[cfg(feature = "neo-term")]
pub use bootstrap::run_render_loop_current_thread_with_terminals;
pub use bootstrap::{build_render_event_loop, run_render_loop, run_render_loop_current_thread};
use state::{FpsCounter, ImeCursorArea, RenderApp};
pub use state::{
    ImageDecodeTerminal, ImageRenderState, ImageTerminalProbe, ImageTerminalPublication,
    MonitorInfo, SharedImageRenderState, SharedMonitorInfo,
};
pub use thread_handle::RenderThread;

use winit::event_loop::EventLoopProxy;

pub(crate) use neomacs_renderer_wgpu::{PopupMenuState, TooltipState};

#[derive(Clone, Copy, Debug)]
pub enum RenderUserEvent {
    Wake,
}

pub type RenderEventLoopProxy = EventLoopProxy<RenderUserEvent>;
pub type RenderEventLoop = winit::event_loop::EventLoop<RenderUserEvent>;

// All GPU caches (image, video, webkit) are managed by WgpuRenderer
