//! Error types for the display engine.

use thiserror::Error;

/// Main error type for display operations
#[derive(Error, Debug)]
pub enum DisplayError {
    #[error("Display engine initialization failed: {0}")]
    InitFailed(String),

    #[error("Backend error: {0}")]
    Backend(String),

    #[error("Rendering error: {0}")]
    Render(String),

    #[error("Invalid glyph: {0}")]
    InvalidGlyph(String),

    #[error("Image loading failed: {0}")]
    ImageLoad(String),

    #[error("Video error: {0}")]
    Video(String),

    #[error("WebKit error: {0}")]
    WebKit(String),

    #[error("Font error: {0}")]
    Font(String),

    #[error("FFI error: {0}")]
    Ffi(String),
}

/// Result type alias
pub type DisplayResult<T> = Result<T, DisplayError>;

#[cfg(test)]
#[path = "error/tests/error_test.rs"]
mod tests;
