pub(in crate::buffer) mod backend;
#[cfg(not(any(test, feature = "test-support")))]
mod edit;
#[cfg(any(test, feature = "test-support"))]
pub mod edit;
mod emacs_bytes;
mod kind;
mod layout;
mod metrics;
mod snapshot;

#[cfg(not(any(test, feature = "test-support")))]
pub(in crate::buffer) use edit::TextExtentDelta;
#[cfg(any(test, feature = "test-support"))]
pub use edit::TextExtentDelta;
pub use edit::{
    TextChange, TextEditRange, TextExtent, TextInsertion, TextReplacement, TextTransposition,
};
#[cfg(feature = "test-support")]
pub use emacs_bytes::emacs_char_to_byte_in_slice;
#[cfg(not(feature = "test-support"))]
pub(in crate::buffer) use emacs_bytes::{
    emacs_byte_to_char_in_slice, emacs_char_to_byte_in_slice, is_emacs_char_boundary,
    storage_string_to_emacs_buffer_bytes,
};
#[cfg(feature = "test-support")]
pub(in crate::buffer) use emacs_bytes::{
    emacs_byte_to_char_in_slice, is_emacs_char_boundary, storage_string_to_emacs_buffer_bytes,
};
pub use kind::BufferTextBackendKind;
#[doc(hidden)]
pub use kind::ImplementedBufferTextBackendKind;
#[cfg(not(any(test, feature = "test-support")))]
pub(in crate::buffer) use layout::GapCompatState;
#[cfg(any(test, feature = "test-support"))]
pub use layout::GapCompatState;
#[cfg(test)]
pub(in crate::buffer) use layout::{GapDebugLayout, TextBackendDebugLayout};
pub use metrics::TextMetrics;
#[doc(hidden)]
pub use snapshot::BufferTextBytesSnapshot;

#[cfg_attr(feature = "test-support", visibility::make(pub))]
#[inline]
pub(crate) fn emacs_char_count_bytes(bytes: &[u8], multibyte: bool) -> crate::buffer::CharLen {
    crate::buffer::CharLen::new(if multibyte {
        neovm_value::emacs_char::chars_in_multibyte(bytes)
    } else {
        bytes.len()
    })
}
