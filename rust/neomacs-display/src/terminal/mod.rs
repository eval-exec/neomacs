//! Neo-term: GPU-accelerated terminal emulator for Neomacs.
//!
//! Uses `alacritty_terminal` for VT parsing and terminal state,
//! renders cells directly via the wgpu pipeline.

pub mod colors;
pub mod content;
pub mod view;

pub use content::TerminalContent;
pub use view::{TerminalManager, TerminalView};

/// Unique identifier for a terminal instance.
pub type TerminalId = u32;

/// Shared terminal state accessible from both Emacs and render threads.
/// Maps terminal ID to its Arc<FairMutex<Term>> for cross-thread text extraction.
pub type SharedTerminals = std::sync::Arc<
    std::sync::Mutex<
        std::collections::HashMap<
            TerminalId,
            std::sync::Arc<parking_lot::FairMutex<alacritty_terminal::term::Term<view::NeomacsEventProxy>>>,
        >,
    >,
>;

/// Terminal display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalMode {
    /// Terminal fills an entire Emacs window/buffer.
    Window,
    /// Terminal is inline within buffer text (like an inline image).
    Inline,
    /// Terminal floats on top of all content (renderer-level compositing).
    Floating,
}
