//! Every neomacs-renderer-wgpu integration suite in one test binary.
//!
//! Each former per-file binary linked the crate dependency closure again;
//! nextest runs every test in its own process, so one binary is equivalent
//! at run time.

#[path = "headless_probe.rs"]
mod headless_probe;

#[path = "offscreen_frame.rs"]
mod offscreen_frame;

#[path = "shader_surface_compose.rs"]
mod shader_surface_compose;

#[path = "window_chrome.rs"]
mod window_chrome;
