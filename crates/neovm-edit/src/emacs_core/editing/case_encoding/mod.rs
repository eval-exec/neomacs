//! Buffer encoding projection shared by the editor and text case operations.
//!
//! This is plain call-local data: whether integer character codes 128..255 are
//! Latin-1 or raw bytes. The constructor that reads the current buffer lives
//! with the text-side casing operations above this layer.

/// Buffer encoding determines whether integer characters 128..255 denote
/// Latin-1 or raw bytes. This call-local projection contains no shared state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum CaseEncoding {
    Unibyte,
    Multibyte,
}
