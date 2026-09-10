//! Convert Android argument conventions before entering the owned VM protocol.

use neovm_host_abi::ime::{ImeSelection, ImeTextSnapshot, ImeUtf16Offset};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DecodeError {
    InvalidSelection,
}

/// GNU androidterm.c forwards start as point and end as mark. Offsets here
/// are relative to the exported excerpt, not the whole editor buffer.
/// Reject invalid offsets rather than guessing at an editor position.
pub(super) fn selection(
    snapshot: &ImeTextSnapshot,
    start: i32,
    end: i32,
) -> Result<ImeSelection, DecodeError> {
    let cursor = ImeUtf16Offset::new(start).ok_or(DecodeError::InvalidSelection)?;
    let anchor = ImeUtf16Offset::new(end).ok_or(DecodeError::InvalidSelection)?;
    snapshot
        .selection_from_utf16(cursor, anchor)
        .ok_or(DecodeError::InvalidSelection)
}

#[cfg(test)]
#[path = "tests/decode.rs"]
mod tests;
