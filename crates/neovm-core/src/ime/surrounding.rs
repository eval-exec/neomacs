//! VM-owned surrounding text: bounded UTF-8 observations with stored position mapping.

use crate::buffer::EmacsBytePos;
use neovm_host_abi::ime::{ImeSnapshotId, ImeTextSnapshot};

#[derive(Debug, Default)]
pub(super) struct SurroundingTextState {
    next: u64,
    pub(super) captured: Option<CapturedText>,
}

impl SurroundingTextState {
    pub(super) fn invalidate(&mut self) {
        self.captured = None;
    }
}

#[derive(Debug)]
pub(super) struct CapturedText {
    pub(super) source: super::InsertionAnchor,
    pub(super) id: ImeSnapshotId,
    pub(super) positions: Vec<(usize, EmacsBytePos)>,
}

impl crate::Context {
    /// Observe a bounded excerpt around point and active mark on the VM thread.
    ///
    /// Respects narrowing. An oversized selection or a selected non-Unicode
    /// Emacs character cannot be represented and returns `None`. Non-Unicode
    /// surrounding characters terminate the excerpt instead of being replaced
    /// lossily. Each observation retires the previous identity, even on failure.
    pub fn ime_surrounding_text(&mut self) -> Option<ImeTextSnapshot> {
        self.composition.surrounding.captured = None;
        let source = self.ime_anchor()?;
        let buffer = self.buffers.current_buffer()?;
        let point = source.point;
        let mark = source.selection_anchor.unwrap_or(point);
        let mut start = point.min(mark);
        let mut end = point.max(mark);
        if start < buffer.point_min_emacs_byte_pos() || end > buffer.point_max_emacs_byte_pos() {
            return None;
        }

        let mut selected = Vec::new();
        let mut bytes = 0;
        let mut pos = start;
        while pos < end {
            let ch = buffer.char_after_emacs_byte_pos(pos)?;
            bytes += ch.len_utf8();
            if bytes > ImeTextSnapshot::MAX_BYTES {
                return None;
            }
            selected.push((pos, ch));
            pos = pos.add_len(buffer.char_after_emacs_byte_len(pos)?);
        }

        let before_budget = (ImeTextSnapshot::MAX_BYTES - bytes) / 2;
        let mut before_bytes = 0;
        let mut characters = Vec::new();
        while start > buffer.point_min_emacs_byte_pos() {
            let Some(ch) = buffer.char_before_emacs_byte_pos(start) else {
                break;
            };
            if before_bytes + ch.len_utf8() > before_budget {
                break;
            }
            start = start.saturating_sub_len(buffer.char_before_emacs_byte_len(start)?);
            characters.push((start, ch));
            before_bytes += ch.len_utf8();
        }
        characters.reverse();
        characters.extend(selected);
        bytes += before_bytes;
        while end < buffer.point_max_emacs_byte_pos() {
            let Some(ch) = buffer.char_after_emacs_byte_pos(end) else {
                break;
            };
            if bytes + ch.len_utf8() > ImeTextSnapshot::MAX_BYTES {
                break;
            }
            characters.push((end, ch));
            bytes += ch.len_utf8();
            end = end.add_len(buffer.char_after_emacs_byte_len(end)?);
        }

        let mut text = String::with_capacity(bytes);
        let mut positions = Vec::with_capacity(characters.len() + 1);
        for (pos, ch) in characters {
            positions.push((text.len(), pos));
            text.push(ch);
        }
        positions.push((text.len(), end));
        let cursor = positions.iter().find(|(_, pos)| *pos == point)?.0;
        let anchor = positions.iter().find(|(_, pos)| *pos == mark)?.0;
        self.composition.surrounding.next = self.composition.surrounding.next.checked_add(1)?;
        let snapshot = ImeTextSnapshot::new(
            ImeSnapshotId(self.composition.surrounding.next),
            text,
            cursor,
            anchor,
        )?;
        self.composition.surrounding.captured = Some(CapturedText {
            source,
            id: snapshot.id(),
            positions,
        });
        Some(snapshot)
    }
}
