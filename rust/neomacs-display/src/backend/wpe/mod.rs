//! WPE WebKit backend for headless browser rendering.
//!
//! This module provides WPE WebKit integration for embedding web content
//! in Emacs buffers using the WPE Platform API for GPU-accelerated rendering.
//!
//! Architecture:
//! - WPE Platform API: Modern display/view/buffer abstraction
//! - wpe-webkit: WebKit engine (GObject API)
//! - dma-buf: Zero-copy GPU buffer sharing

#[cfg(feature = "wpe-webkit")]
mod sys;

#[cfg(feature = "wpe-webkit")]
mod platform;

#[cfg(feature = "wpe-webkit")]
mod backend;

#[cfg(feature = "wpe-webkit")]
mod view;

#[cfg(feature = "wpe-webkit")]
mod dmabuf;

#[cfg(feature = "wpe-webkit")]
pub use backend::WpeBackend;

#[cfg(feature = "wpe-webkit")]
pub use view::{WpeWebView, WpeViewState};

#[cfg(feature = "wpe-webkit")]
pub use dmabuf::DmaBufExporter;

#[cfg(feature = "wpe-webkit")]
pub use platform::{WpePlatformDisplay, WpePlatformView};
