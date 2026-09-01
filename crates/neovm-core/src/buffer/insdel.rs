//! Structural buffer edit pipeline.
//!
//! This module is the first source-ownership extraction toward a GNU
//! `insdel.c`-style boundary. It rehomes the existing `Buffer` edit core
//! without changing behavior.

use super::{Buffer, BufferId, BufferManager};
use crate::buffer::edit_transaction::{
    DeletionString, InsertMarkerAdjustment, InsertMarkerPlacement, InsertTextPlan,
    MeasuredDeleteEdit, MeasuredInsertEdit, MeasuredReplaceEdit, MeasuredSameLenEdit,
    ReplaceTextPlan, SameLenModifiedStatePolicy, SameLenSubstitutionPlan, SharedTextEditMetadata,
    SharedTextEditOutcome, TranspositionStoragePlan,
};
#[cfg(test)]
use crate::buffer::position::EmacsByteLen;
use crate::buffer::{
    CharRange, EmacsBytePos, EmacsByteRange, TextEditRange, TextExtent, TextInsertion,
    TextReplacement, TextTransposition,
};
use crate::heap_types::LispString;

impl Buffer {
    fn edit_range_at_emacs_byte_pos(&self, byte_pos: EmacsBytePos) -> TextEditRange {
        self.text.edit_range_at_emacs_byte_pos(byte_pos)
    }

    pub fn edit_range_for_emacs_byte_range(&self, byte_range: EmacsByteRange) -> TextEditRange {
        if byte_range.is_empty() {
            return self.edit_range_at_emacs_byte_pos(byte_range.start());
        }
        self.text.edit_range_for_emacs_byte_range(byte_range)
    }

    pub fn edit_range_for_char_range(&self, char_range: CharRange) -> TextEditRange {
        if char_range.is_empty() {
            let byte_pos = self.text.char_pos_to_emacs_byte_pos(char_range.start());
            return TextEditRange::empty_at(byte_pos, char_range.start());
        }
        self.text.edit_range_for_char_range(char_range)
    }

    pub fn text_transposition_for_char_ranges(
        &self,
        first: CharRange,
        second: CharRange,
    ) -> TextTransposition {
        TextTransposition::new(
            self.edit_range_for_char_range(first),
            self.edit_range_for_char_range(second),
        )
    }

    /// Insert `text` at point, advancing point past the inserted text.
    ///
    /// Markers at the insertion site move according to their `InsertionType`.
    /// Returns the measured insertion so callers can update sibling-buffer
    /// bookkeeping without re-measuring the storage-form input.
    fn insert_internal(
        &mut self,
        text: &str,
        marker_placement: InsertMarkerPlacement,
    ) -> TextInsertion {
        if text.is_empty() {
            return TextInsertion::at_anchor(self.point_anchor(), TextExtent::ZERO);
        }
        let plan = InsertTextPlan::from_storage_text(
            text,
            self.get_multibyte(),
            self.point_anchor(),
            marker_placement,
            InsertMarkerAdjustment::ByInsertionType,
        );
        self.execute_insert_text_plan(plan)
    }

    pub fn insert(&mut self, text: &str) -> TextInsertion {
        self.insert_internal(text, InsertMarkerPlacement::AfterMarkers)
    }

    pub fn insert_before_markers(&mut self, text: &str) -> TextInsertion {
        self.insert_internal(text, InsertMarkerPlacement::BeforeMarkers)
    }

    pub fn insert_lisp_string(&mut self, text: &LispString) -> TextInsertion {
        self.insert_lisp_string_full(
            text,
            InsertMarkerPlacement::AfterMarkers,
            InsertMarkerAdjustment::ByInsertionType,
        )
    }

    pub fn insert_lisp_string_before_markers(&mut self, text: &LispString) -> TextInsertion {
        self.insert_lisp_string_full(
            text,
            InsertMarkerPlacement::BeforeMarkers,
            InsertMarkerAdjustment::ByInsertionType,
        )
    }

    /// GNU-equivalent replace path: insert `text` at point but do NOT
    /// advance markers exactly at the insertion site even if their
    /// `insertion_type` is true. This matches GNU
    /// `adjust_markers_for_replace` (insdel.c:341), where markers at
    /// `from_byte` stay put regardless of insertion_type.
    pub fn insert_lisp_string_for_replace(&mut self, text: &LispString) -> TextInsertion {
        self.insert_lisp_string_full(
            text,
            InsertMarkerPlacement::AfterMarkers,
            InsertMarkerAdjustment::StrictAfter,
        )
    }

    fn insert_lisp_string_full(
        &mut self,
        text: &LispString,
        marker_placement: InsertMarkerPlacement,
        marker_adjustment: InsertMarkerAdjustment,
    ) -> TextInsertion {
        let plan = InsertTextPlan::from_lisp_string(
            text,
            self.get_multibyte(),
            self.point_anchor(),
            marker_placement,
            marker_adjustment,
        );
        self.execute_insert_text_plan(plan)
    }

    pub fn replace_emacs_byte_range_lisp_string(
        &mut self,
        byte_range: EmacsByteRange,
        text: &LispString,
    ) -> TextReplacement {
        if byte_range.start() > byte_range.end() {
            return TextReplacement::default();
        }
        let old_range = self.edit_range_for_emacs_byte_range(byte_range);
        self.replace_measured_region_lisp_string(old_range, text)
    }

    pub fn replace_measured_region_lisp_string(
        &mut self,
        old_range: TextEditRange,
        text: &LispString,
    ) -> TextReplacement {
        let plan = ReplaceTextPlan::from_lisp_string(old_range, text, self.get_multibyte());
        self.execute_replace_text_plan(plan)
    }

    fn replace_measured_region_lisp_string_edit(
        &mut self,
        old_range: TextEditRange,
        text: &LispString,
    ) -> MeasuredReplaceEdit {
        MeasuredReplaceEdit::new(self.replace_measured_region_lisp_string(old_range, text))
    }

    /// Delete an Emacs-byte range.
    ///
    /// Adjusts point, mark, markers, and the narrowing boundary.
    pub fn delete_emacs_byte_range(&mut self, byte_range: EmacsByteRange) -> TextEditRange {
        if byte_range.is_empty() {
            return TextEditRange::default();
        }
        let range = self.edit_range_for_emacs_byte_range(byte_range);
        self.delete_measured_region(range)
    }

    pub fn delete_measured_region(&mut self, range: TextEditRange) -> TextEditRange {
        if range.is_empty() {
            return TextEditRange::default();
        }
        let plan = self.delete_text_plan_for_range(range);
        self.execute_delete_text_plan(plan)
    }

    fn delete_measured_region_edit(&mut self, range: TextEditRange) -> MeasuredDeleteEdit {
        MeasuredDeleteEdit::new(self.delete_measured_region(range))
    }

    /// Delete a measured range and return the deleted text with its
    /// properties -- GNU `del_range_1 (..., ret_string = true)`: the one
    /// string that is recorded for undo (when enabled) and handed back.
    pub fn delete_and_extract_measured_region(
        &mut self,
        range: TextEditRange,
    ) -> (MeasuredDeleteEdit, LispString) {
        let plan = self.delete_text_plan_for_range_extracting(range, DeletionString::Wanted);
        let (edit_range, text) = self.execute_delete_text_plan_extracting(plan);
        (
            MeasuredDeleteEdit::new(edit_range),
            text.expect("DeletionString::Wanted always builds the deleted text"),
        )
    }

    /// Replace every occurrence of `from_code` with the Emacs-encoded
    /// bytes in `to_bytes` in the measured range.
    ///
    /// The replacement is performed in place, so callers must ensure the
    /// matched character's Emacs-byte length equals `to_bytes.len()`.
    pub fn subst_char_in_region(
        &mut self,
        range: TextEditRange,
        modified_range: TextEditRange,
        from_code: u32,
        to_bytes: &[u8],
        noundo: bool,
    ) -> bool {
        let edit = MeasuredSameLenEdit::new(range, modified_range);
        if edit.is_empty() {
            return false;
        }

        let mut region_bytes = Vec::with_capacity(range.byte_len().get());
        self.text
            .copy_emacs_byte_range_to(range.byte_range(), &mut region_bytes);
        let Some(plan) = SameLenSubstitutionPlan::new(
            range,
            &region_bytes,
            self.get_multibyte(),
            from_code,
            to_bytes,
        ) else {
            return false;
        };

        self.execute_same_len_substitution_plan(range, modified_range, &region_bytes, plan, noundo);
        true
    }

    pub fn subst_char_changed_range(
        &self,
        range: TextEditRange,
        from_code: u32,
        to_bytes: &[u8],
    ) -> Option<TextEditRange> {
        if range.byte_range().is_empty() {
            return None;
        }

        let mut region_bytes = Vec::with_capacity(range.byte_len().get());
        self.text
            .copy_emacs_byte_range_to(range.byte_range(), &mut region_bytes);

        SameLenSubstitutionPlan::new(
            range,
            &region_bytes,
            self.get_multibyte(),
            from_code,
            to_bytes,
        )
        .map(|plan| plan.first_to_last_changed_range())
    }

    /// GNU `Ftranspose_regions` core: swap two non-overlapping current-buffer
    /// regions without changing buffer size.  Text movement is byte-based,
    /// while property and marker movement follows GNU's character positions.
    pub fn transpose_regions(&mut self, transposition: TextTransposition, leave_markers: bool) {
        let first = transposition.first();
        let second = transposition.second();
        let mut region1 = Vec::with_capacity(first.byte_len().get());
        let mut mid = Vec::with_capacity(transposition.middle_byte_range().len().get());
        let mut region2 = Vec::with_capacity(second.byte_len().get());
        self.text
            .copy_emacs_byte_range_to(first.byte_range(), &mut region1);
        self.text
            .copy_emacs_byte_range_to(transposition.middle_byte_range(), &mut mid);
        self.text
            .copy_emacs_byte_range_to(second.byte_range(), &mut region2);

        let plan = TranspositionStoragePlan::new(transposition, &region1, &mid, &region2);
        self.execute_transposition_storage_plan(plan, leave_markers);
    }
}

/// Structural text mutation entry points for buffers and indirect-buffer
/// siblings. This is the closest Rust ownership boundary to GNU `insdel.c`.
impl BufferManager {
    pub fn edit_range_for_buffer_emacs_byte_range(
        &self,
        id: BufferId,
        byte_range: EmacsByteRange,
    ) -> Option<TextEditRange> {
        self.buffers
            .get(&id)
            .map(|buf| buf.edit_range_for_emacs_byte_range(byte_range))
    }

    pub fn edit_range_for_buffer_char_range(
        &self,
        id: BufferId,
        char_range: CharRange,
    ) -> Option<TextEditRange> {
        self.buffers
            .get(&id)
            .map(|buf| buf.edit_range_for_char_range(char_range))
    }

    pub fn insert_into_buffer(&mut self, id: BufferId, text: &str) -> Option<()> {
        if text.is_empty() {
            return Some(());
        }
        self.execute_shared_text_edit(id, |buffer| {
            let insertion = buffer.insert(text);
            let edit = MeasuredInsertEdit::by_insertion_type(
                insertion,
                InsertMarkerPlacement::AfterMarkers,
            );
            Some(SharedTextEditOutcome::edited(
                (),
                SharedTextEditMetadata::Insert(edit),
            ))
        })
    }

    pub fn insert_lisp_string_into_buffer(
        &mut self,
        id: BufferId,
        text: &LispString,
    ) -> Option<()> {
        self.insert_lisp_string_into_buffer_full(id, text, InsertMarkerAdjustment::ByInsertionType)
    }

    /// GNU-equivalent replace path: like `insert_lisp_string_into_buffer`
    /// but doesn't push markers exactly at point past the inserted text,
    /// even if their `insertion_type` is true. Used by
    /// `replace_buffer_region_lisp_string_in_manager` to match GNU
    /// `adjust_markers_for_replace` (insdel.c:341) semantics.
    pub fn insert_lisp_string_into_buffer_for_replace(
        &mut self,
        id: BufferId,
        text: &LispString,
    ) -> Option<()> {
        self.insert_lisp_string_into_buffer_full(id, text, InsertMarkerAdjustment::StrictAfter)
    }

    fn insert_lisp_string_into_buffer_full(
        &mut self,
        id: BufferId,
        text: &LispString,
        marker_adjustment: InsertMarkerAdjustment,
    ) -> Option<()> {
        if text.is_empty() {
            return Some(());
        }
        self.execute_shared_text_edit(id, |buffer| {
            let insertion = if marker_adjustment == InsertMarkerAdjustment::StrictAfter {
                buffer.insert_lisp_string_for_replace(text)
            } else {
                buffer.insert_lisp_string(text)
            };
            let edit = MeasuredInsertEdit::new(
                insertion,
                InsertMarkerPlacement::AfterMarkers,
                marker_adjustment,
            );
            Some(SharedTextEditOutcome::edited(
                (),
                SharedTextEditMetadata::Insert(edit),
            ))
        })
    }

    pub fn insert_into_buffer_before_markers(&mut self, id: BufferId, text: &str) -> Option<()> {
        if text.is_empty() {
            return Some(());
        }
        self.execute_shared_text_edit(id, |buffer| {
            let insertion = buffer.insert_before_markers(text);
            let edit = MeasuredInsertEdit::by_insertion_type(
                insertion,
                InsertMarkerPlacement::BeforeMarkers,
            );
            Some(SharedTextEditOutcome::edited(
                (),
                SharedTextEditMetadata::Insert(edit),
            ))
        })
    }

    pub fn insert_lisp_string_into_buffer_before_markers(
        &mut self,
        id: BufferId,
        text: &LispString,
    ) -> Option<()> {
        if text.is_empty() {
            return Some(());
        }
        self.execute_shared_text_edit(id, |buffer| {
            let insertion = buffer.insert_lisp_string_before_markers(text);
            let edit = MeasuredInsertEdit::by_insertion_type(
                insertion,
                InsertMarkerPlacement::BeforeMarkers,
            );
            Some(SharedTextEditOutcome::edited(
                (),
                SharedTextEditMetadata::Insert(edit),
            ))
        })
    }

    pub fn delete_buffer_emacs_byte_range(
        &mut self,
        id: BufferId,
        byte_range: EmacsByteRange,
    ) -> Option<()> {
        if byte_range.is_empty() {
            return Some(());
        }
        let range = self.edit_range_for_buffer_emacs_byte_range(id, byte_range)?;
        self.delete_buffer_measured_region(id, range)
    }

    pub fn delete_buffer_char_range(&mut self, id: BufferId, char_range: CharRange) -> Option<()> {
        if char_range.is_empty() {
            return Some(());
        }
        let range = self.edit_range_for_buffer_char_range(id, char_range)?;
        self.delete_buffer_measured_region(id, range)
    }

    pub fn delete_buffer_measured_region(
        &mut self,
        id: BufferId,
        range: TextEditRange,
    ) -> Option<()> {
        if range.is_empty() {
            return Some(());
        }

        self.execute_shared_text_edit(id, |buffer| {
            let edit = buffer.delete_measured_region_edit(range);
            Some(SharedTextEditOutcome::edited(
                (),
                SharedTextEditMetadata::Delete(edit),
            ))
        })
    }

    /// `delete-and-extract-region`'s core: delete `range` from buffer `id`
    /// and return the deleted text (GNU `del_range_1` with `ret_string`);
    /// `None` for an empty range or a missing buffer.
    pub fn delete_and_extract_buffer_measured_region(
        &mut self,
        id: BufferId,
        range: TextEditRange,
    ) -> Option<LispString> {
        if range.is_empty() {
            return None;
        }

        self.execute_shared_text_edit(id, |buffer| {
            let (edit, text) = buffer.delete_and_extract_measured_region(range);
            Some(SharedTextEditOutcome::edited(
                text,
                SharedTextEditMetadata::Delete(edit),
            ))
        })
    }

    #[cfg(test)]
    pub fn replace_buffer_region_lisp_string(
        &mut self,
        id: BufferId,
        start: usize,
        end: usize,
        text: &LispString,
    ) -> Option<()> {
        if start > end {
            return None;
        }
        self.replace_buffer_emacs_byte_range_lisp_string(
            id,
            EmacsByteRange::from_start_len(
                EmacsBytePos::new(start),
                EmacsByteLen::new(end - start),
            ),
            text,
        )
    }

    pub fn replace_buffer_emacs_byte_range_lisp_string(
        &mut self,
        id: BufferId,
        byte_range: EmacsByteRange,
        text: &LispString,
    ) -> Option<()> {
        if byte_range.start() > byte_range.end() {
            return None;
        }
        let range = self.edit_range_for_buffer_emacs_byte_range(id, byte_range)?;
        self.replace_buffer_measured_region_lisp_string(id, range, text)
    }

    pub fn replace_buffer_measured_region_lisp_string(
        &mut self,
        id: BufferId,
        range: TextEditRange,
        text: &LispString,
    ) -> Option<()> {
        // GNU: `if (nbytes_del <= 0 && inschars == 0) return;` (insdel.c:1521).
        if range.is_empty() && text.is_empty() {
            return Some(());
        }

        // Every other shape, including an empty old range, is one
        // `replace_range`.  There is deliberately no insert-only branch here:
        // the sibling metadata for a replacement already delegates an empty
        // old range to the insertion behaviour it shares with GNU
        // (`adjust_markers_for_replace_range` and
        // `adjust_for_replace_at_emacs_byte_pos` both dispatch on
        // `old_len.is_empty()`), so a separate insert path would buy nothing
        // and could only reintroduce the missing-deletion bug of
        // DIVERGENCES.md 47.
        self.execute_shared_text_edit(id, |buffer| {
            let edit = buffer.replace_measured_region_lisp_string_edit(range, text);
            Some(SharedTextEditOutcome::edited(
                (),
                SharedTextEditMetadata::Replace(edit),
            ))
        })
    }

    /// Replace `byte_range` with `text` using GNU `casify_region`'s undo
    /// recording (a single `record_delete` of the original text followed by a
    /// `record_insert`).  Used by `upcase-region`/`downcase-region`/
    /// `capitalize-region` so the undo list shape matches GNU even when the
    /// replacement leaves the text unchanged.
    pub fn casify_replace_buffer_emacs_byte_range_lisp_string(
        &mut self,
        id: BufferId,
        byte_range: EmacsByteRange,
        text: &LispString,
    ) -> Option<()> {
        if byte_range.start() >= byte_range.end() {
            return None;
        }
        let range = self.edit_range_for_buffer_emacs_byte_range(id, byte_range)?;
        if range.is_empty() {
            return None;
        }
        let multibyte = self.buffers.get(&id)?.get_multibyte();
        let plan = ReplaceTextPlan::from_lisp_string(range, text, multibyte);
        self.execute_shared_text_edit(id, |buffer| {
            let replacement = buffer.execute_casify_replace_text_plan(plan);
            let edit = MeasuredReplaceEdit::new(replacement);
            Some(SharedTextEditOutcome::edited(
                (),
                SharedTextEditMetadata::Replace(edit),
            ))
        })
    }

    pub fn subst_char_in_buffer_region(
        &mut self,
        id: BufferId,
        range: TextEditRange,
        modified_range: TextEditRange,
        from_code: u32,
        to_bytes: &[u8],
        noundo: bool,
    ) -> Option<bool> {
        if range.byte_range().is_empty() {
            return Some(false);
        }

        let edit = MeasuredSameLenEdit::new(range, modified_range);
        self.execute_shared_text_edit(id, |buffer| {
            let changed =
                buffer.subst_char_in_region(range, modified_range, from_code, to_bytes, noundo);
            if !changed {
                return Some(SharedTextEditOutcome::no_edit(false));
            }
            Some(SharedTextEditOutcome::edited(
                true,
                SharedTextEditMetadata::SameLen {
                    edit,
                    modified_state: SameLenModifiedStatePolicy::RecordChange,
                },
            ))
        })
    }

    pub fn transpose_buffer_regions(
        &mut self,
        id: BufferId,
        transposition: TextTransposition,
        leave_markers: bool,
    ) -> Option<()> {
        self.execute_shared_text_edit(id, |buffer| {
            buffer.transpose_regions(transposition, leave_markers);
            let edit = MeasuredSameLenEdit::covering(transposition.span_edit_range());
            Some(SharedTextEditOutcome::edited(
                (),
                SharedTextEditMetadata::Transposition {
                    edit,
                    transposition,
                    modified_state: SameLenModifiedStatePolicy::RecordChange,
                },
            ))
        })
    }
}
