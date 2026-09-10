//! Convert Android argument conventions before entering the owned VM protocol.

use neovm_host_abi::ime::{ImeSelection, ImeTextSnapshot, ImeUtf16Offset};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DecodeError {
    InvalidSelection,
    InvalidDeletion,
}

/// Android measures these offsets in UTF-16 units relative to the replacement
/// start/end, not relative to point before the edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CursorPlacement {
    BeforeStart(u32),
    AfterEnd(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextEditKind {
    Commit,
    Compose,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TextEdit {
    pub(super) kind: TextEditKind,
    pub(super) text: String,
    pub(super) cursor: CursorPlacement,
}

pub(super) fn text_edit(kind: TextEditKind, text: String, cursor: i32) -> TextEdit {
    let cursor = if cursor > 0 {
        CursorPlacement::AfterEnd((cursor - 1) as u32)
    } else {
        CursorPlacement::BeforeStart(cursor.unsigned_abs())
    };
    TextEdit { kind, text, cursor }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeletionUnit {
    Utf16,
    CodePoint,
}

/// Platform counts, not UTF-8 byte ranges. Resolve only against the exact
/// text and selection associated with the incoming operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Deletion {
    pub(super) before: u32,
    pub(super) after: u32,
    pub(super) unit: DeletionUnit,
}

pub(super) fn deletion(
    before: i32,
    after: i32,
    unit: DeletionUnit,
) -> Result<Deletion, DecodeError> {
    Ok(Deletion {
        before: before
            .try_into()
            .map_err(|_| DecodeError::InvalidDeletion)?,
        after: after.try_into().map_err(|_| DecodeError::InvalidDeletion)?,
        unit,
    })
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
