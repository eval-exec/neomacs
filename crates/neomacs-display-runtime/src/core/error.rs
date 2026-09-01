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
mod tests {
    use super::*;

    #[test]
    fn display_init_failed() {
        let err = DisplayError::InitFailed("no GPU found".into());
        assert_eq!(
            err.to_string(),
            "Display engine initialization failed: no GPU found"
        );
    }

    #[test]
    fn display_backend() {
        let err = DisplayError::Backend("TTY not ready".into());
        assert_eq!(err.to_string(), "Backend error: TTY not ready");
    }

    #[test]
    fn display_render() {
        let err = DisplayError::Render("out of memory".into());
        assert_eq!(err.to_string(), "Rendering error: out of memory");
    }

    #[test]
    fn display_invalid_glyph() {
        let err = DisplayError::InvalidGlyph("missing codepoint".into());
        assert_eq!(err.to_string(), "Invalid glyph: missing codepoint");
    }

    #[test]
    fn display_image_load() {
        let err = DisplayError::ImageLoad("corrupt PNG".into());
        assert_eq!(err.to_string(), "Image loading failed: corrupt PNG");
    }

    #[test]
    fn display_video() {
        let err = DisplayError::Video("codec unsupported".into());
        assert_eq!(err.to_string(), "Video error: codec unsupported");
    }

    #[test]
    fn display_webkit() {
        let err = DisplayError::WebKit("context failed".into());
        assert_eq!(err.to_string(), "WebKit error: context failed");
    }

    #[test]
    fn display_font() {
        let err = DisplayError::Font("missing glyph".into());
        assert_eq!(err.to_string(), "Font error: missing glyph");
    }

    #[test]
    fn display_ffi() {
        let err = DisplayError::Ffi("null pointer".into());
        assert_eq!(err.to_string(), "FFI error: null pointer");
    }

    #[test]
    fn debug_format() {
        let err = DisplayError::Render("test".into());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Render"));
        assert!(debug.contains("test"));
    }

    #[test]
    fn error_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<DisplayError>();
        assert_sync::<DisplayError>();
    }

    #[test]
    fn implements_std_error() {
        let err = DisplayError::InitFailed("boom".into());
        let std_err: &dyn std::error::Error = &err;
        assert!(std_err.source().is_none());
    }

    #[test]
    fn display_result_ok() {
        let result: DisplayResult<i32> = Ok(42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn display_result_err() {
        let result: DisplayResult<i32> = Err(DisplayError::Backend("fail".into()));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Backend error: fail");
    }

    #[test]
    fn variants_are_distinguishable() {
        let err = DisplayError::Font("x".into());
        assert!(matches!(err, DisplayError::Font(_)));
        assert!(!matches!(err, DisplayError::Ffi(_)));
        assert!(!matches!(err, DisplayError::Render(_)));
    }
}
