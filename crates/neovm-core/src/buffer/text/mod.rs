pub(in crate::buffer) mod backend;
mod edit;
mod emacs_bytes;
mod kind;
mod layout;
mod metrics;
mod snapshot;

pub(in crate::buffer) use edit::TextExtentDelta;
pub use edit::{
    TextChange, TextEditRange, TextExtent, TextInsertion, TextReplacement, TextTransposition,
};
pub(in crate::buffer) use emacs_bytes::{
    emacs_byte_to_char_in_slice, emacs_char_to_byte_in_slice, is_emacs_char_boundary,
    storage_string_to_emacs_buffer_bytes,
};
pub use kind::BufferTextBackendKind;
pub(crate) use kind::ImplementedBufferTextBackendKind;
pub(in crate::buffer) use layout::GapCompatState;
#[cfg(test)]
pub(in crate::buffer) use layout::{GapDebugLayout, TextBackendDebugLayout};
pub use metrics::TextMetrics;
pub(crate) use snapshot::BufferTextBytesSnapshot;

#[inline]
pub(crate) fn emacs_char_count_bytes(bytes: &[u8], multibyte: bool) -> crate::buffer::CharLen {
    crate::buffer::CharLen::new(if multibyte {
        crate::emacs_core::emacs_char::chars_in_multibyte(bytes)
    } else {
        bytes.len()
    })
}
