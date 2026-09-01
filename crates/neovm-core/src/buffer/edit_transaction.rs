//! GNU-shaped buffer edit transaction policy.
//!
//! This module owns the semantic side-effect policy and current-buffer
//! transaction executors used by the structural edit pipeline.  Public edit
//! entry points still live in `insdel.rs`, while this module keeps the
//! GNU-shaped insert/delete/replace ordering in one place.

use super::{Buffer, BufferManager};
use crate::buffer::text::TextExtentDelta;
use crate::buffer::undo;
use crate::buffer::{
    BufferId, BufferText, CharLen, CharPos0, EmacsByteLen, EmacsBytePos, EmacsByteRange,
    TextEditRange, TextExtent, TextInsertion, TextPositionAnchor, TextPropertyTable,
    TextReplacement, TextTransposition,
};
use crate::emacs_core::value::Value;
use crate::heap_types::LispString;

#[inline]
pub(in crate::buffer) fn lisp_string_from_buffer_bytes(
    bytes: Vec<u8>,
    multibyte: bool,
) -> LispString {
    if multibyte {
        LispString::from_emacs_bytes(bytes)
    } else {
        LispString::from_unibyte(bytes)
    }
}

#[inline]
pub(in crate::buffer) fn char_pos_for_emacs_byte(
    text: &BufferText,
    byte_pos: EmacsBytePos,
) -> CharPos0 {
    text.emacs_byte_pos_to_char_pos(byte_pos)
}

#[inline]
pub(in crate::buffer) fn emacs_byte_for_char_pos(
    text: &BufferText,
    char_pos: CharPos0,
) -> EmacsBytePos {
    text.char_pos_to_emacs_byte_pos(char_pos)
}

#[inline]
pub(in crate::buffer) fn encode_char_code_for_buffer_bytes(code: u32, multibyte: bool) -> Vec<u8> {
    if multibyte {
        let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = crate::emacs_core::emacs_char::char_string(code, &mut buf);
        buf[..len].to_vec()
    } else {
        assert!(
            code <= 0xFF,
            "unibyte insertion produced non-byte character code {code:#X}"
        );
        vec![code as u8]
    }
}

pub(in crate::buffer) fn convert_lisp_string_for_buffer_mode(
    text: &LispString,
    target_multibyte: bool,
) -> LispString {
    if text.is_multibyte() == target_multibyte {
        return text.clone();
    }

    let mut codes = crate::emacs_core::builtins::lisp_string_char_codes(text);
    if target_multibyte {
        for code in &mut codes {
            if *code > 0x7F {
                *code = crate::emacs_core::emacs_char::unibyte_to_char(*code as u8);
            }
        }
    } else {
        for code in &mut codes {
            *code &= 0xFF;
        }
    }

    let mut bytes = Vec::new();
    for code in codes {
        bytes.extend_from_slice(&encode_char_code_for_buffer_bytes(code, target_multibyte));
    }
    let mut converted = lisp_string_from_buffer_bytes(bytes, target_multibyte);
    if text.has_intervals() {
        let intervals = text.intervals().clone();
        if !intervals.is_empty() {
            *converted.intervals_mut() = intervals;
        }
    }
    converted
}

impl Buffer {
    fn edit_state(&self) -> BufferEditState {
        BufferEditState::new(
            self.point_anchor(),
            self.point_min_anchor(),
            self.point_max_anchor(),
        )
    }

    fn set_edit_state(&mut self, state: BufferEditState) {
        self.set_accessible_region_anchors_unchecked(state.begv(), state.zv());
        self.set_point_anchor_unchecked(state.point());
    }

    pub(in crate::buffer) fn buffer_region_lisp_string(&self, range: EmacsByteRange) -> LispString {
        let mut bytes = Vec::new();
        self.text.copy_emacs_byte_range_to(range, &mut bytes);
        let mut string = lisp_string_from_buffer_bytes(bytes, self.get_multibyte());
        let props = self.text.text_props_slice_emacs_byte_range(range);
        if !props.is_empty() {
            *string.intervals_mut() = props;
        }
        string
    }

    pub(in crate::buffer) fn delete_text_plan_for_range(
        &self,
        range: TextEditRange,
    ) -> DeleteTextPlan {
        self.delete_text_plan_for_range_extracting(range, DeletionString::IfRecorded)
    }

    /// GNU `del_range_2` (src/insdel.c:2023): the deleted text becomes a
    /// string only `if (ret_string || ! EQ (BVAR (current_buffer, undo_list),
    /// Qt))` -- a caller that wants it back, or an undo list that will record
    /// it.  A buffer with undo disabled (`with-temp-buffer`, kill-ring
    /// staging) otherwise paid a full text-property slice per delete for a
    /// string nothing read.
    pub(in crate::buffer) fn delete_text_plan_for_range_extracting(
        &self,
        range: TextEditRange,
        want: DeletionString,
    ) -> DeleteTextPlan {
        let build = matches!(want, DeletionString::Wanted)
            || !undo::undo_list_is_disabled(&self.get_undo_list());
        DeleteTextPlan::new(
            range,
            build.then(|| self.buffer_region_lisp_string(range.byte_range())),
            self.text.marker_adjustments_for_delete(range),
        )
    }

    /// Execute a fully measured insertion plan.  The replacement path uses
    /// [`InsertMarkerAdjustment::StrictAfter`] so markers collapsed to the
    /// replacement start are not pushed past the inserted text, matching GNU
    /// `adjust_markers_for_replace` (insdel.c:341).
    pub(in crate::buffer) fn execute_insert_text_plan(
        &mut self,
        plan: InsertTextPlan,
    ) -> TextInsertion {
        let edit = plan.edit();
        if edit.is_empty() {
            return edit.insertion();
        }
        // GNU `record_insert` always calls `record_point`, and that path
        // records the first-change sentinel when the buffer was unmodified.
        self.undo_prepare_change(edit.char_pos());
        let mut ul = self.get_undo_list();
        if !undo::undo_list_is_disabled(&ul) {
            undo::undo_list_record_insert(&mut ul, edit.char_pos(), edit.char_len());
            self.set_undo_list(ul);
        }
        self.apply_insert_text_plan(plan)
    }

    /// Mutate storage for a fully measured insertion plan *without* recording
    /// undo.
    ///
    /// Split out from [`Self::execute_insert_text_plan`] so that recording and
    /// mutation are separate layers: an insertion records
    /// `record_insert`, whereas a replacement whose old range is empty is
    /// still storage-wise an insertion but records GNU `replace_range`'s
    /// delete-and-insert pair. The caller owns the recording; this owns the
    /// text.
    fn apply_insert_text_plan(&mut self, plan: InsertTextPlan) -> TextInsertion {
        let edit = plan.edit();
        if edit.is_empty() {
            return edit.insertion();
        }
        self.text
            .insert_measured_emacs_bytes(edit.byte_pos(), plan.bytes(), edit.extent());
        self.apply_byte_insert_side_effects(edit, InsertSideEffectPolicy::current_buffer());
        if edit.before_markers() {
            self.text
                .advance_markers_at_position(edit.byte_pos(), edit.extent());
        }
        if let Some(text_properties) = plan.text_properties() {
            self.text
                .text_props_append_shifted_at_emacs_byte_pos(text_properties, edit.byte_pos());
        }
        edit.insertion()
    }

    /// Execute a fully measured deletion plan.
    ///
    /// GNU `del_range_2` records deletion undo before marker adjustment and
    /// storage mutation.  The plan owns the pre-mutation deleted text and
    /// marker-adjustment undo entries so this executor consumes one
    /// transaction object instead of recomputing side inputs mid-edit.
    pub(in crate::buffer) fn execute_delete_text_plan(
        &mut self,
        plan: DeleteTextPlan,
    ) -> TextEditRange {
        self.execute_delete_text_plan_extracting(plan).0
    }

    /// Execute a delete plan and hand back its deleted text (present when
    /// the plan was built with [`DeletionString::Wanted`] or undo recorded
    /// it) -- GNU `del_range_2`'s `deletion`, which is both recorded and
    /// returned from one `make_buffer_string_both`.
    pub(in crate::buffer) fn execute_delete_text_plan_extracting(
        &mut self,
        mut plan: DeleteTextPlan,
    ) -> (TextEditRange, Option<LispString>) {
        let edit = plan.edit();
        if edit.is_empty() {
            return (edit.range(), plan.take_deleted_text());
        }

        // GNU `record_delete` (undo.c) runs `record_point` FIRST, then the
        // marker adjustments, then conses the deletion, so the point entry
        // precedes the `(MARKER . ADJ)` entries (GNU bug 16818 ordering).
        self.undo_prepare_change(plan.range().char_start());
        let mut ul = self.get_undo_list();
        if !undo::undo_list_is_disabled(&ul) {
            for &(marker, adjustment) in plan.marker_adjustments() {
                undo::undo_list_record_marker_adjustment(&mut ul, marker, adjustment);
            }
            // Undo was disabled when the plan was made only if something
            // enabled it since; build the record's text then.
            let deleted = plan
                .deleted_text()
                .cloned()
                .unwrap_or_else(|| self.buffer_region_lisp_string(plan.range().byte_range()));
            undo::undo_list_record_delete(
                &mut ul,
                plan.range().char_start(),
                deleted,
                self.point_char_pos(),
            );
            self.set_undo_list(ul);
        }

        self.text.delete_measured_range(plan.range());
        self.apply_byte_delete_side_effects(edit, DeleteSideEffectPolicy::current_buffer());
        (plan.range(), plan.take_deleted_text())
    }

    /// Execute a fully measured replacement plan.
    ///
    /// This is the single entry point for GNU `replace_range` (insdel.c:1463).
    /// Recording happens once, unconditionally, before any dispatch on the
    /// plan's shape, because GNU records once unconditionally: `deletion` is
    /// `make_buffer_string_both (from, ..., to, ...)`, a *string* that is
    /// merely empty when `from == to`, so `!NILP (deletion)` always holds and
    /// `record_insert` / `record_delete` always both run (insdel.c:1614-1618).
    /// Only the storage mutation varies with the shape.
    ///
    /// Keeping the recording ahead of the dispatch is deliberate: it is what
    /// makes an insert-only replacement — the class of bug behind
    /// DIVERGENCES.md 47, where a pure-insertion change run recorded no
    /// deletion and two adjacent runs then coalesced into one undo record —
    /// unrepresentable here rather than merely fixed.
    pub(in crate::buffer) fn execute_replace_text_plan(
        &mut self,
        plan: ReplaceTextPlan,
    ) -> TextReplacement {
        let old_range = plan.old_range();

        if old_range.is_empty() && plan.new_extent().chars().is_empty() {
            // GNU: `if (nbytes_del <= 0 && inschars == 0) return;`
            // (insdel.c:1521) — returns before recording anything.
            return TextReplacement::new(old_range, plan.new_extent());
        }

        self.record_replace_range_undo(&plan);

        if old_range.is_empty() {
            // GNU `adjust_markers_for_replace` delegates `old_chars == 0`
            // straight to `adjust_markers_for_insert` (insdel.c:351), so the
            // storage mutation, markers, overlays and point all behave exactly
            // as for `insert`.  Only the recording above differs, and it has
            // already happened.
            let insertion_plan = plan.into_insert_plan(
                old_range.start_anchor(),
                InsertMarkerPlacement::AfterMarkers,
                InsertMarkerAdjustment::ByInsertionType,
            );
            let insertion = self.apply_insert_text_plan(insertion_plan);
            debug_assert_eq!(old_range.byte_start(), insertion.byte_pos());
            debug_assert_eq!(old_range.char_start(), insertion.char_pos());
            return TextReplacement::new(old_range, insertion.extent());
        }

        let replacement = plan.replacement();
        self.text.replace_measured_range(replacement, plan.bytes());
        self.apply_replace_side_effects(
            MeasuredReplaceEdit::new(replacement),
            ReplaceSideEffectPolicy::current_buffer(),
        );
        if let Some(text_properties) = plan.text_properties() {
            self.text.text_props_append_shifted_at_emacs_byte_pos(
                text_properties,
                replacement.byte_start(),
            );
        } else if !plan.new_extent().chars().is_empty() {
            self.text.text_props_set_properties_in_emacs_byte_range(
                EmacsByteRange::from_start_len(
                    replacement.byte_start(),
                    plan.new_extent().emacs_bytes(),
                ),
                Vec::new(),
            );
        }
        replacement
    }

    /// Record GNU `replace_range`'s undo pair for `plan`, whatever its shape.
    ///
    /// GNU records the insertion first and the deletion second
    /// (insdel.c:1610-1618): "Record the insertion first, so that when we
    /// undo, the deletion will be undone first.  Thus, undo will insert
    /// before deleting, and thus will keep the markers before and after this
    /// text separate."  Because entries cons onto the front, the deletion
    /// ends up newest.
    ///
    /// Nothing here inspects a length.  A pure-insertion run records a
    /// zero-length deletion `("" . POS)` and a pure-deletion run records a
    /// zero-length insertion `(POS . POS)`, exactly as GNU does, because
    /// `record_delete` and `record_insert` (undo.c) return early only for a
    /// disabled undo list.  Those zero-length entries are load-bearing:
    /// `record_insert` coalesces into the newest record only when that record
    /// is a `(FIXNUM . FIXNUM)` cons whose CDR equals the new start
    /// (undo.c:100-112), so a record of the other shape between two adjacent
    /// runs is what keeps them from merging into one span.
    fn record_replace_range_undo(&mut self, plan: &ReplaceTextPlan) {
        let old_range = plan.old_range();
        // GNU reads `deletion` and tests point before it touches the gap, so
        // both see the pre-edit buffer.
        let old_point = self.point_anchor();
        let deleted_text = self.buffer_region_lisp_string(old_range.byte_range());

        // GNU `replace_range` (insdel.c:1638-1639) records the insertion
        // first, so the `record_point` prologue runs for the *insertion's*
        // beg -- `from + SCHARS (deletion)`, i.e. the old range's end -- and
        // the following `record_delete` finds the list off a boundary and adds
        // nothing.
        self.undo_prepare_change(plan.old_char_end());
        let mut ul = self.get_undo_list();
        if undo::undo_list_is_disabled(&ul) {
            return;
        }
        undo::undo_list_record_insert(&mut ul, plan.old_char_end(), plan.new_char_len());
        undo::undo_list_record_delete(
            &mut ul,
            plan.old_char_start(),
            deleted_text,
            old_point.char_pos(),
        );
        self.set_undo_list(ul);
    }

    /// Execute a case-region replacement using GNU `casify_region`'s undo
    /// shape (casefiddle.c:529): `record_delete (start, ORIGINAL_TEXT, false)`
    /// followed by `record_insert (start, NEW_LEN)`.
    ///
    /// Unlike [`Self::execute_replace_text_plan`] this records the deletion of
    /// the original text *before* the insertion (so the final undo list is
    /// `((START . START+NEWLEN) (ORIGINAL . START) POINT ...)`), records the
    /// insert at `start` rather than at the old end, and records undo even when
    /// the replacement is identical to the original (GNU always runs
    /// `record_delete`/`record_insert` because the case op `modify_text`s the
    /// range first).  Marker adjustments are *not* recorded — GNU passes
    /// `record_markers = false` because the case operation preserves marker
    /// positions for a same-length change.
    pub(in crate::buffer) fn execute_casify_replace_text_plan(
        &mut self,
        plan: ReplaceTextPlan,
    ) -> TextReplacement {
        let old_range = plan.old_range();
        debug_assert!(
            !old_range.is_empty(),
            "casify replace requires a non-empty range"
        );

        let old_point = self.point_anchor();
        let deleted_text = self.buffer_region_lisp_string(old_range.byte_range());

        self.undo_prepare_change(old_range.char_start());
        let mut ul = self.get_undo_list();
        if !undo::undo_list_is_disabled(&ul) {
            // GNU `casify_region` records `record_delete` first, then
            // `record_insert`, so `primitive-undo` reinserts the original text
            // after deleting the recased text.
            undo::undo_list_record_delete(
                &mut ul,
                old_range.char_start(),
                deleted_text,
                old_point.char_pos(),
            );
            undo::undo_list_record_insert(&mut ul, old_range.char_start(), plan.new_char_len());
            self.set_undo_list(ul);
        }

        let replacement = plan.replacement();
        if replacement.old_byte_len() == replacement.new_byte_len() {
            // Same byte length: GNU `casify_region` overwrites the bytes in
            // place (`memcpy`) without moving point, markers, or overlays.  Use
            // the same-length mutation path so interior markers are preserved.
            self.text
                .replace_same_len_measured_range(replacement, plan.bytes());
            self.apply_same_len_edit_side_effects(
                MeasuredSameLenEdit::covering(replacement.old_range()),
                SameLenModifiedStatePolicy::RecordChange,
            );
        } else {
            // Byte length changed (e.g. a Unicode case mapping with a different
            // encoding length): fall back to the general replacement so markers
            // and overlays past the change are shifted.
            self.text.replace_measured_range(replacement, plan.bytes());
            self.apply_replace_side_effects(
                MeasuredReplaceEdit::new(replacement),
                ReplaceSideEffectPolicy::current_buffer(),
            );
        }
        if let Some(text_properties) = plan.text_properties() {
            self.text.text_props_append_shifted_at_emacs_byte_pos(
                text_properties,
                replacement.byte_start(),
            );
        } else if !plan.new_extent().chars().is_empty() {
            self.text.text_props_set_properties_in_emacs_byte_range(
                EmacsByteRange::from_start_len(
                    replacement.byte_start(),
                    plan.new_extent().emacs_bytes(),
                ),
                Vec::new(),
            );
        }
        replacement
    }

    fn apply_byte_insert_side_effects(
        &mut self,
        edit: MeasuredInsertEdit,
        policy: InsertSideEffectPolicy,
    ) {
        if edit.is_empty() {
            return;
        }
        let insertion = edit.insertion();
        let char_len = edit.char_len();

        self.set_edit_state(edit.state_after(self.edit_state(), policy));
        if policy.shared_side_data.adjust_in_this_buffer() {
            if edit.marker_adjustment() == InsertMarkerAdjustment::StrictAfter {
                self.text
                    .adjust_markers_for_insert_extent_strict_after(edit.byte_pos(), edit.extent());
            } else {
                self.text
                    .adjust_markers_for_insert_extent(edit.byte_pos(), edit.extent());
            }
        }
        debug_assert_eq!(
            char_pos_for_emacs_byte(&self.text, edit.byte_pos()),
            edit.char_pos(),
            "insert-side-effect char position drifted from the source edit site"
        );
        if policy.shared_side_data.adjust_in_this_buffer() {
            self.text
                .adjust_text_props_for_insert_at(insertion.char_pos(), insertion.extent().chars());
        }
        self.overlays
            .adjust_for_inserted_text(insertion, edit.marker_placement().before_markers());
        let insert_char_pos = insertion.char_pos().get() as i64;
        self.record_char_modification(char_len);
        // Phase 3 unchanged-region accumulator: an insert is a zero-width change
        // at `pos` (everything from `pos` shifts right). `old_z` is the char
        // count before this insert.
        let new_z = self.text.char_count().get() as i64;
        let inserted = char_len.get() as i64;
        self.text
            .note_changed_char_region(insert_char_pos, insert_char_pos, new_z - inserted);
    }

    fn apply_byte_delete_side_effects(
        &mut self,
        edit: MeasuredDeleteEdit,
        policy: DeleteSideEffectPolicy,
    ) {
        let range = edit.range();
        if edit.is_empty() {
            return;
        }
        let char_len = edit.char_len();

        self.set_edit_state(edit.state_after(self.edit_state(), policy));

        if policy.shared_side_data.adjust_in_this_buffer() {
            self.text.adjust_markers_for_delete_range(range);
        }

        if policy.shared_side_data.adjust_in_this_buffer() {
            self.text
                .adjust_text_props_for_delete_range(range.char_range());
        }
        self.overlays.adjust_for_deleted_text(range);
        let delete_char_start = range.char_range().start().get() as i64;
        self.record_char_modification(char_len);
        // Phase 3 unchanged-region accumulator: a delete of [a, a+M) leaves a
        // change at `a` (the deleted span is gone). old_z = new_z + M.
        let new_z = self.text.char_count().get() as i64;
        let deleted = char_len.get() as i64;
        self.text.note_changed_char_region(
            delete_char_start,
            delete_char_start + deleted,
            new_z + deleted,
        );
    }

    pub(in crate::buffer) fn apply_same_len_edit_side_effects(
        &mut self,
        edit: MeasuredSameLenEdit,
        modified_state: SameLenModifiedStatePolicy,
    ) {
        if edit.is_empty() {
            return;
        }
        let old_state = self.modified_state_value();
        let modified = edit.modified_range();
        let changed_start = modified.char_start().get() as i64;
        let changed_end = modified.char_end().get() as i64;
        self.record_char_modification(edit.changed_chars());
        // Phase 3 unchanged-region accumulator: a same-length substitution
        // changes chars [start, end) IN PLACE — nothing shifts, so old_z == the
        // current (unchanged) char count. Lets overwrite-mode / subst-char edits
        // take the localized-edit fast path instead of a full rebuild.
        let new_z = self.text.char_count().get() as i64;
        self.text
            .note_changed_char_region(changed_start, changed_end, new_z);
        if modified_state.preserve_unmodified_if_clean() && old_state.is_nil() {
            self.text.set_save_modified_tick(self.text.modified_tick());
        }
    }

    /// Execute a same-length substitution over an already scanned range.
    ///
    /// GNU `subst-char-in-region` records each changed character as a
    /// delete+insert pair before overwriting the bytes in place.  `noundo`
    /// suppresses that undo recording but still applies the same storage
    /// change through the normal same-length side-effect path.
    pub(in crate::buffer) fn execute_same_len_substitution_plan(
        &mut self,
        range: TextEditRange,
        modified_range: TextEditRange,
        region_bytes: &[u8],
        plan: SameLenSubstitutionPlan,
        noundo: bool,
    ) {
        let edit = MeasuredSameLenEdit::new(range, modified_range);
        if !noundo {
            self.undo_prepare_change(modified_range.char_start());
            let mut ul = self.get_undo_list();
            if !undo::undo_list_is_disabled(&ul) {
                for changed_range in plan.changed_ranges().iter().copied() {
                    let mut deleted = lisp_string_from_buffer_bytes(
                        region_bytes[changed_range.byte_index_range_relative_to(range)].to_vec(),
                        self.get_multibyte(),
                    );
                    let props = self
                        .text
                        .text_props_slice_emacs_byte_range(changed_range.byte_range());
                    if !props.is_empty() {
                        *deleted.intervals_mut() = props;
                    }
                    undo::undo_list_record_delete(
                        &mut ul,
                        changed_range.char_start(),
                        deleted,
                        self.point_char_pos(),
                    );
                    undo::undo_list_record_insert(
                        &mut ul,
                        changed_range.char_start(),
                        changed_range.char_len(),
                    );
                }
                self.set_undo_list(ul);
            }
        }

        self.text.replace_same_len_measured_range(
            plan.replacement_for_range(range, self.get_multibyte()),
            plan.replacement_bytes(),
        );
        self.apply_same_len_edit_side_effects(edit, SameLenModifiedStatePolicy::RecordChange);
    }

    /// Record GNU-style undo entries for `transpose-regions`.
    ///
    /// GNU records a same-length adjacent transposition as one changed span,
    /// non-adjacent equal-length transpositions as two independent changed
    /// regions, and unequal-length transpositions as one full span.
    pub(in crate::buffer) fn record_transposition_undo(
        &mut self,
        transposition: TextTransposition,
    ) {
        let first = transposition.first();
        let second = transposition.second();
        let old_span = self.buffer_region_lisp_string(transposition.byte_span());

        self.undo_prepare_change(first.char_start());
        let mut undo_list = self.get_undo_list();
        if undo::undo_list_is_disabled(&undo_list) {
            return;
        }

        let record_change =
            |undo_list: &mut Value, start_char: CharPos0, deleted: LispString, pt: CharPos0| {
                let len_chars = CharLen::new(deleted.schars());
                undo::undo_list_record_delete(undo_list, start_char, deleted, pt);
                undo::undo_list_record_insert(undo_list, start_char, len_chars);
            };

        if transposition.same_char_len() {
            if transposition.adjacent() {
                record_change(
                    &mut undo_list,
                    first.char_start(),
                    old_span,
                    self.point_char_pos(),
                );
            } else {
                record_change(
                    &mut undo_list,
                    first.char_start(),
                    self.buffer_region_lisp_string(first.byte_range()),
                    self.point_char_pos(),
                );
                record_change(
                    &mut undo_list,
                    second.char_start(),
                    self.buffer_region_lisp_string(second.byte_range()),
                    self.point_char_pos(),
                );
            }
        } else {
            record_change(
                &mut undo_list,
                first.char_start(),
                old_span,
                self.point_char_pos(),
            );
        }
        self.set_undo_list(undo_list);
    }

    fn transpose_region_properties(&self, transposition: TextTransposition) -> TextPropertyTable {
        let first = transposition.first();
        let second = transposition.second();
        let props1 = self
            .text
            .text_props_snapshot()
            .slice_char_range(first.char_range());
        let props2 = self
            .text
            .text_props_snapshot()
            .slice_char_range(second.char_range());
        let props_mid = if transposition.same_char_len() {
            TextPropertyTable::new()
        } else {
            self.text
                .text_props_snapshot()
                .slice_char_range(transposition.middle_char_range())
        };

        let mut props = self.text.text_props_snapshot();
        if transposition.same_char_len() {
            props.remove_all_properties_in_char_range(first.char_range());
            props.remove_all_properties_in_char_range(second.char_range());
        } else {
            props.remove_all_properties_in_char_range(transposition.char_span());
            props.append_shifted_at_char_pos(
                &props_mid,
                transposition.middle_destination_char_start(),
            );
        }
        props.append_shifted_at_char_pos(&props1, transposition.first_destination_char_start());
        props.append_shifted_at_char_pos(&props2, transposition.second_destination_char_start());
        props
    }

    /// Execute the current-buffer storage, property, marker, and point portion
    /// of GNU `transpose-regions` after the byte movement plan has been built.
    pub(in crate::buffer) fn execute_transposition_storage_plan(
        &mut self,
        plan: TranspositionStoragePlan,
        leave_markers: bool,
    ) {
        let transposition = plan.transposition();
        let first = transposition.first();
        let second = transposition.second();

        self.record_transposition_undo(transposition);

        let replacement_props = self.transpose_region_properties(transposition);
        if transposition.same_char_len() {
            self.set_text_properties_with_undo_range(first.byte_range(), Vec::new());
            self.set_text_properties_with_undo_range(second.byte_range(), Vec::new());
        } else {
            self.set_text_properties_with_undo_range(transposition.byte_span(), Vec::new());
        }
        let new_point = transposition.transpose_anchor(self.point_anchor());

        self.text
            .replace_same_len_measured_range(plan.replacement(), plan.replacement_bytes());
        self.text.text_props_replace(replacement_props);
        if leave_markers {
            self.text.remap_marker_anchors(|old_position| {
                let old_byte = old_position.emacs_byte_pos();
                if old_byte > first.byte_start() && old_byte <= second.byte_end() {
                    TextPositionAnchor::new(
                        old_position.char_pos(),
                        emacs_byte_for_char_pos(&self.text, old_position.char_pos()),
                    )
                } else {
                    old_position
                }
            });
        } else {
            self.text
                .remap_marker_anchors(|old_position| transposition.transpose_anchor(old_position));
        }

        self.set_point_anchor_unchecked(new_point);
        self.apply_same_len_edit_side_effects(
            plan.edit(),
            SameLenModifiedStatePolicy::RecordChange,
        );
    }

    fn apply_replace_side_effects(
        &mut self,
        edit: MeasuredReplaceEdit,
        policy: ReplaceSideEffectPolicy,
    ) {
        if edit.is_empty() {
            return;
        }

        let replacement = edit.replacement();
        let old_range = replacement.old_range();

        self.set_edit_state(edit.state_after(self.edit_state(), policy));
        if policy.shared_side_data.adjust_in_this_buffer() {
            self.text
                .adjust_markers_for_replace_range(old_range, edit.new_extent());
        }
        if policy.shared_side_data.adjust_in_this_buffer() {
            self.text.adjust_text_props_for_replace_at(
                edit.old_char_start(),
                edit.old_char_len(),
                edit.new_char_len(),
            );
        }
        self.overlays.adjust_for_replaced_text(replacement);
        self.record_char_modification(edit.changed_chars());
    }

    pub(in crate::buffer) fn apply_shared_text_edit_side_effects(
        &mut self,
        edit: SharedTextEditMetadata,
        state_policy: SharedTextEditStatePolicy,
    ) {
        match edit {
            SharedTextEditMetadata::Insert(edit) => self.apply_byte_insert_side_effects(
                edit,
                InsertSideEffectPolicy::shared_buffer(state_policy.structural_state_update()),
            ),
            SharedTextEditMetadata::Delete(edit) => self.apply_byte_delete_side_effects(
                edit,
                DeleteSideEffectPolicy::shared_buffer(state_policy.structural_state_update()),
            ),
            SharedTextEditMetadata::Replace(edit) => self.apply_replace_side_effects(
                edit,
                ReplaceSideEffectPolicy::shared_buffer(state_policy.structural_state_update()),
            ),
            SharedTextEditMetadata::SameLen {
                edit,
                modified_state,
            } => self.apply_same_len_edit_side_effects(edit, modified_state),
            SharedTextEditMetadata::Transposition {
                edit,
                transposition,
                modified_state,
            } => {
                if state_policy.structural_state_update().update_state_fields() {
                    let point = transposition.transpose_anchor(self.point_anchor());
                    self.set_point_anchor_unchecked(point);
                }
                self.apply_same_len_edit_side_effects(edit, modified_state);
            }
        }
    }

    fn record_char_modification(&mut self, changed_chars: CharLen) {
        self.text
            .record_char_modification(modification_tick_delta(changed_chars));
    }

    /// GNU `record_point` (undo.c:47-78), the prologue every recorder runs
    /// exactly once before it conses anything for a change starting at `beg`.
    ///
    /// GNU's body is three steps *in this order*, and the order is the whole
    /// point:
    ///
    /// ```c
    ///   at_boundary = ! CONSP (undo_list) || NILP (XCAR (undo_list));
    ///   if (MODIFF <= SAVE_MODIFF) record_first_change ();
    ///   if (at_boundary && point_before_last_command_or_undo != beg && ...)
    ///     push point_before_last_command_or_undo;
    /// ```
    ///
    /// `at_boundary` is read from the list as the command found it — *before*
    /// `record_first_change` may cons `(t . TIME)` onto it.  Deriving it after
    /// that push answers `false` for exactly the case it exists to catch (the
    /// first change to a clean buffer), so the point entry is dropped and
    /// `primitive-undo` has nothing to restore point from: `undo` leaves point
    /// at the change instead of where the command started.  GNU's own comment
    /// on the check ("currently dependent on being called before
    /// record_first_change") names the hazard.
    ///
    /// Keeping all three steps here, instead of leaving each recorder to redo
    /// the boundary test, is why the ordering cannot be got wrong per call
    /// site: `undo_list_record_insert` / `undo_list_record_delete` no longer
    /// take the saved point at all, so there is nothing for them to record
    /// late.  It also puts the point entry ahead of any `(MARKER . ADJUSTMENT)`
    /// entries a deletion conses (GNU bug 16818 ordering) for free.
    ///
    /// The first-change step gates purely on the modified-tick comparison (no
    /// sticky "already recorded" flag), like GNU's `MODIFF <= SAVE_MODIFF`:
    /// `(set-buffer-modified-p nil)` resets `SAVE_MODIFF = MODIFF`, so the
    /// buffer becomes clean again and the next modification must re-emit
    /// `(t . MODTIME)`.
    pub(in crate::buffer) fn undo_prepare_change(&mut self, beg: CharPos0) {
        let mut ul = self.get_undo_list();
        if undo::undo_list_is_disabled(&ul) {
            return;
        }
        let at_boundary = undo::undo_list_at_boundary(&ul);
        if self.modified_tick() <= self.save_modified_tick() {
            undo::undo_list_record_first_change(&mut ul, self.first_change_modtime());
            self.undo_state.set_recorded_first_change(true);
        }
        // GNU `record_point` (src/undo.c:73-78) takes the saved point only when
        // it is still the SAME buffer: `buffer_before_last_command_or_undo ==
        // current_buffer`.  There is one saved point for the whole editor, so
        // this asks a real question -- the last command may have run in the
        // minibuffer, in a base buffer, or in an indirect buffer over the same
        // text -- and `point_saved_in` answers it without ever handing out a
        // point that is not ours.
        if let Some(saved_point) = self.saved_point_before_command.point_saved_in(self.id)
            && at_boundary
            && saved_point != beg
        {
            undo::undo_list_record_point(&mut ul, saved_point);
        }
        self.set_undo_list(ul);
    }
}

impl BufferManager {
    pub(in crate::buffer) fn execute_shared_text_edit<T>(
        &mut self,
        edited_id: BufferId,
        edit_current_buffer: impl FnOnce(&mut Buffer) -> Option<SharedTextEditOutcome<T>>,
    ) -> Option<T> {
        let scope = self.shared_text_edit_scope(edited_id)?;
        let outcome = {
            let edited_buffer = self.buffer_mut(edited_id)?;
            edit_current_buffer(edited_buffer)?
        };
        if let Some(edit) = outcome.edit {
            self.apply_shared_text_edit_to_siblings(scope, edit)?;
        }
        Some(outcome.result)
    }

    pub(in crate::buffer) fn shared_text_edit_scope(
        &self,
        edited_id: BufferId,
    ) -> Option<SharedTextEditScope> {
        let root_id = self.shared_text_root_id(edited_id)?;
        Some(SharedTextEditScope::new(
            edited_id,
            self.buffers_sharing_root_ids(root_id),
        ))
    }

    fn shared_sibling_state_update(&self, sibling_id: BufferId) -> SharedBufferStateUpdate {
        if self.current_buffer_id() == Some(sibling_id)
            || !self.buffer_has_state_markers(sibling_id)
        {
            SharedBufferStateUpdate::UpdateFields
        } else {
            SharedBufferStateUpdate::RefreshFromStateMarkers
        }
    }

    pub(in crate::buffer) fn apply_shared_text_edit_to_siblings(
        &mut self,
        scope: SharedTextEditScope,
        edit: SharedTextEditMetadata,
    ) -> Option<()> {
        for sibling_id in scope.siblings() {
            let state_policy = edit
                .state_policy_for_shared_sibling(|| self.shared_sibling_state_update(sibling_id));
            {
                let sibling = self.buffer_mut(sibling_id)?;
                sibling.apply_shared_text_edit_side_effects(edit, state_policy);
            }
            if let Some(state_update) = state_policy.state_update() {
                self.refresh_shared_buffer_state_cache(sibling_id, state_update)?;
            }
        }
        Some(())
    }

    fn refresh_shared_buffer_state_cache(
        &mut self,
        buffer_id: BufferId,
        state_update: SharedBufferStateUpdate,
    ) -> Option<()> {
        if state_update.needs_state_marker_refresh() {
            self.fetch_buffer_state_markers(buffer_id)?;
        }
        Some(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) struct BufferEditState {
    point: TextPositionAnchor,
    begv: TextPositionAnchor,
    zv: TextPositionAnchor,
}

impl BufferEditState {
    pub(in crate::buffer) const fn new(
        point: TextPositionAnchor,
        begv: TextPositionAnchor,
        zv: TextPositionAnchor,
    ) -> Self {
        Self { point, begv, zv }
    }

    pub(in crate::buffer) const fn point(self) -> TextPositionAnchor {
        self.point
    }

    pub(in crate::buffer) const fn begv(self) -> TextPositionAnchor {
        self.begv
    }

    pub(in crate::buffer) const fn zv(self) -> TextPositionAnchor {
        self.zv
    }

    fn set_point(&mut self, point: TextPositionAnchor) {
        self.point = point;
    }

    fn set_begv(&mut self, begv: TextPositionAnchor) {
        self.begv = begv;
    }

    fn set_zv(&mut self, zv: TextPositionAnchor) {
        self.zv = zv;
    }
}

fn anchor_byte_gt(anchor: TextPositionAnchor, other: TextPositionAnchor) -> bool {
    anchor.emacs_byte_pos() > other.emacs_byte_pos()
}

fn anchor_byte_ge(anchor: TextPositionAnchor, other: TextPositionAnchor) -> bool {
    anchor.emacs_byte_pos() >= other.emacs_byte_pos()
}

fn anchor_byte_eq(anchor: TextPositionAnchor, other: TextPositionAnchor) -> bool {
    anchor.emacs_byte_pos() == other.emacs_byte_pos()
}

fn move_after_insert(
    position: TextPositionAnchor,
    insertion: TextInsertion,
    move_at_insertion: bool,
) -> TextPositionAnchor {
    let start = insertion.start_anchor();
    if anchor_byte_gt(position, start) || (move_at_insertion && anchor_byte_eq(position, start)) {
        TextExtentDelta::insertion(insertion.extent()).apply_to_anchor(position)
    } else {
        position
    }
}

fn move_after_delete(
    position: TextPositionAnchor,
    range: TextEditRange,
    move_at_end: bool,
) -> TextPositionAnchor {
    let start = range.start_anchor();
    let end = range.end_anchor();
    if anchor_byte_gt(position, end) || (move_at_end && anchor_byte_eq(position, end)) {
        TextExtentDelta::deletion(range.extent()).apply_to_anchor(position)
    } else if anchor_byte_gt(position, start) {
        start
    } else {
        position
    }
}

fn move_after_replace_for_point(
    position: TextPositionAnchor,
    replacement: TextReplacement,
) -> TextPositionAnchor {
    let old_range = replacement.old_range();
    let start = replacement.old_start_anchor();
    let end = replacement.old_end_anchor();

    if anchor_byte_gt(position, start) && position.emacs_byte_pos() < end.emacs_byte_pos()
        || anchor_byte_eq(position, end)
    {
        TextExtentDelta::insertion(replacement.new_extent()).apply_to_anchor(start)
    } else if anchor_byte_gt(position, end) {
        TextExtentDelta::replacement(old_range.extent(), replacement.new_extent())
            .apply_to_anchor(position)
    } else {
        position
    }
}

fn move_after_replace_for_begv(
    position: TextPositionAnchor,
    replacement: TextReplacement,
) -> TextPositionAnchor {
    let old_range = replacement.old_range();
    let start = replacement.old_start_anchor();
    let end = replacement.old_end_anchor();

    if anchor_byte_gt(position, end) {
        TextExtentDelta::replacement(old_range.extent(), replacement.new_extent())
            .apply_to_anchor(position)
    } else if anchor_byte_gt(position, start) {
        start
    } else {
        position
    }
}

fn move_after_replace_for_zv(
    position: TextPositionAnchor,
    replacement: TextReplacement,
) -> TextPositionAnchor {
    let old_range = replacement.old_range();
    let start = replacement.old_start_anchor();
    let end = replacement.old_end_anchor();

    if anchor_byte_ge(position, end) {
        TextExtentDelta::replacement(old_range.extent(), replacement.new_extent())
            .apply_to_anchor(position)
    } else if anchor_byte_gt(position, start) {
        TextExtentDelta::insertion(replacement.new_extent()).apply_to_anchor(start)
    } else {
        position
    }
}

pub(in crate::buffer) fn replace_state_after_edit(
    mut state: BufferEditState,
    replacement: TextReplacement,
) -> BufferEditState {
    state.set_point(move_after_replace_for_point(state.point(), replacement));
    state.set_begv(move_after_replace_for_begv(state.begv(), replacement));
    state.set_zv(move_after_replace_for_zv(state.zv(), replacement));

    state
}

pub(in crate::buffer) fn insert_state_after_edit(
    mut state: BufferEditState,
    insertion: TextInsertion,
    policy: InsertSideEffectPolicy,
) -> BufferEditState {
    if !policy.state_fields.update_state_fields() {
        return state;
    }

    state.set_point(move_after_insert(
        state.point(),
        insertion,
        policy.point_at_insertion.advance_point_at_insertion(),
    ));
    if policy.accessible_start.shift_after_edit() {
        state.set_begv(move_after_insert(state.begv(), insertion, false));
    }
    state.set_zv(move_after_insert(state.zv(), insertion, true));

    state
}

pub(in crate::buffer) fn delete_state_after_edit(
    mut state: BufferEditState,
    range: TextEditRange,
    policy: DeleteSideEffectPolicy,
) -> BufferEditState {
    if !policy.state_fields.update_state_fields() {
        return state;
    }

    state.set_point(move_after_delete(state.point(), range, true));

    if policy.accessible_start.shift_after_edit() {
        state.set_begv(move_after_delete(state.begv(), range, true));
    }
    state.set_zv(move_after_delete(state.zv(), range, true));

    state
}

/// GNU `modiff` increments logarithmically with edit size, and
/// `chars_modiff` is reset to the new `modiff` on each character change.
pub(in crate::buffer) fn modification_tick_delta(changed_chars: CharLen) -> i64 {
    let changed_chars = changed_chars.get();
    if changed_chars == 0 {
        1
    } else {
        changed_chars.ilog2() as i64 + 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) enum InsertMarkerAdjustment {
    ByInsertionType,
    StrictAfter,
}

/// GNU insert marker placement mode.
///
/// GNU passes this as a `before_markers` boolean to `insert_1_both` and
/// `adjust_markers_for_insert`.  Keeping it as an enum at the Rust edit
/// boundary prevents callers from mixing up the marker-placement decision with
/// unrelated boolean side-effect toggles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) enum InsertMarkerPlacement {
    AfterMarkers,
    BeforeMarkers,
}

impl InsertMarkerPlacement {
    pub(in crate::buffer) const fn before_markers(self) -> bool {
        matches!(self, Self::BeforeMarkers)
    }
}

/// A fully measured GNU-style insert operation.
///
/// GNU `insert_1_both` receives the insertion point plus both `nchars` and
/// `nbytes` before touching the gap.  Marker placement is part of the edit
/// operation, not part of a later buffer-state policy, so keep it attached to
/// the measured insertion as it flows through current and indirect buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) struct MeasuredInsertEdit {
    insertion: TextInsertion,
    marker_placement: InsertMarkerPlacement,
    marker_adjustment: InsertMarkerAdjustment,
}

impl MeasuredInsertEdit {
    pub(in crate::buffer) const fn new(
        insertion: TextInsertion,
        marker_placement: InsertMarkerPlacement,
        marker_adjustment: InsertMarkerAdjustment,
    ) -> Self {
        Self {
            insertion,
            marker_placement,
            marker_adjustment,
        }
    }

    pub(in crate::buffer) const fn by_insertion_type(
        insertion: TextInsertion,
        marker_placement: InsertMarkerPlacement,
    ) -> Self {
        Self::new(
            insertion,
            marker_placement,
            InsertMarkerAdjustment::ByInsertionType,
        )
    }

    pub(in crate::buffer) const fn insertion(self) -> TextInsertion {
        self.insertion
    }

    pub(in crate::buffer) const fn is_empty(self) -> bool {
        self.insertion.extent().is_empty()
    }

    pub(in crate::buffer) const fn byte_pos(self) -> EmacsBytePos {
        self.insertion.byte_pos()
    }

    pub(in crate::buffer) const fn char_pos(self) -> CharPos0 {
        self.insertion.char_pos()
    }

    pub(in crate::buffer) const fn extent(self) -> TextExtent {
        self.insertion.extent()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(in crate::buffer) const fn byte_len(self) -> EmacsByteLen {
        self.insertion.extent().emacs_bytes()
    }

    pub(in crate::buffer) const fn char_len(self) -> CharLen {
        self.insertion.extent().chars()
    }

    pub(in crate::buffer) const fn marker_placement(self) -> InsertMarkerPlacement {
        self.marker_placement
    }

    pub(in crate::buffer) const fn marker_adjustment(self) -> InsertMarkerAdjustment {
        self.marker_adjustment
    }

    pub(in crate::buffer) const fn before_markers(self) -> bool {
        self.marker_placement.before_markers()
    }

    pub(in crate::buffer) fn state_after(
        self,
        state: BufferEditState,
        policy: InsertSideEffectPolicy,
    ) -> BufferEditState {
        insert_state_after_edit(state, self.insertion, policy)
    }
}

/// Backend-neutral storage plan for inserting text at a measured buffer point.
///
/// This keeps GNU's insert inputs together before the storage mutation: Emacs
/// bytes, measured character/byte extent, marker placement, and any source
/// text properties that must be grafted onto the inserted range after the
/// structural side effects run.
#[derive(Clone, Debug)]
pub(in crate::buffer) struct InsertTextPlan {
    bytes: Vec<u8>,
    text_properties: TextPropertyTable,
    edit: MeasuredInsertEdit,
}

impl InsertTextPlan {
    pub(in crate::buffer) fn from_storage_text(
        text: &str,
        multibyte: bool,
        anchor: TextPositionAnchor,
        marker_placement: InsertMarkerPlacement,
        marker_adjustment: InsertMarkerAdjustment,
    ) -> Self {
        let bytes =
            crate::emacs_core::string_escape::storage_string_to_buffer_bytes(text, multibyte);
        Self::from_emacs_bytes_at_anchor(
            bytes,
            TextPropertyTable::new(),
            multibyte,
            anchor,
            marker_placement,
            marker_adjustment,
        )
    }

    pub(in crate::buffer) fn from_lisp_string(
        text: &LispString,
        multibyte: bool,
        anchor: TextPositionAnchor,
        marker_placement: InsertMarkerPlacement,
        marker_adjustment: InsertMarkerAdjustment,
    ) -> Self {
        let text = convert_lisp_string_for_buffer_mode(text, multibyte);
        let text_properties = if text.has_intervals() {
            text.intervals().clone()
        } else {
            TextPropertyTable::new()
        };
        Self::from_emacs_bytes_at_anchor(
            text.as_bytes().to_vec(),
            text_properties,
            multibyte,
            anchor,
            marker_placement,
            marker_adjustment,
        )
    }

    fn from_emacs_bytes_at_anchor(
        bytes: Vec<u8>,
        text_properties: TextPropertyTable,
        multibyte: bool,
        anchor: TextPositionAnchor,
        marker_placement: InsertMarkerPlacement,
        marker_adjustment: InsertMarkerAdjustment,
    ) -> Self {
        let extent = TextExtent::from_emacs_bytes(&bytes, multibyte);
        let insertion = TextInsertion::at_anchor(anchor, extent);
        let edit = MeasuredInsertEdit::new(insertion, marker_placement, marker_adjustment);
        Self::from_measured_insert(bytes, text_properties, edit)
    }

    fn from_measured_insert(
        bytes: Vec<u8>,
        text_properties: TextPropertyTable,
        edit: MeasuredInsertEdit,
    ) -> Self {
        Self {
            bytes,
            text_properties,
            edit,
        }
    }

    pub(in crate::buffer) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(in crate::buffer) const fn edit(&self) -> MeasuredInsertEdit {
        self.edit
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(in crate::buffer) const fn insertion(&self) -> TextInsertion {
        self.edit.insertion()
    }

    pub(in crate::buffer) fn text_properties(&self) -> Option<&TextPropertyTable> {
        if self.text_properties.is_empty() {
            None
        } else {
            Some(&self.text_properties)
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::buffer) struct ReplaceTextPlan {
    bytes: Vec<u8>,
    text_properties: TextPropertyTable,
    replacement: TextReplacement,
}

impl ReplaceTextPlan {
    pub(in crate::buffer) fn from_lisp_string(
        old_range: TextEditRange,
        text: &LispString,
        multibyte: bool,
    ) -> Self {
        let text = convert_lisp_string_for_buffer_mode(text, multibyte);
        let text_properties = if text.has_intervals() {
            text.intervals().clone()
        } else {
            TextPropertyTable::new()
        };
        let bytes = text.as_bytes().to_vec();
        let new_extent =
            TextExtent::new(CharLen::new(text.schars()), EmacsByteLen::new(bytes.len()));
        Self {
            bytes,
            text_properties,
            replacement: TextReplacement::new(old_range, new_extent),
        }
    }

    pub(in crate::buffer) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(in crate::buffer) const fn replacement(&self) -> TextReplacement {
        self.replacement
    }

    pub(in crate::buffer) const fn old_range(&self) -> TextEditRange {
        self.replacement.old_range()
    }

    pub(in crate::buffer) const fn old_char_start(&self) -> CharPos0 {
        self.replacement.old_range().char_start()
    }

    pub(in crate::buffer) const fn old_char_end(&self) -> CharPos0 {
        self.replacement.old_range().char_end()
    }

    pub(in crate::buffer) const fn new_extent(&self) -> TextExtent {
        self.replacement.new_extent()
    }

    pub(in crate::buffer) const fn new_char_len(&self) -> CharLen {
        self.replacement.new_char_len()
    }

    pub(in crate::buffer) fn text_properties(&self) -> Option<&TextPropertyTable> {
        if self.text_properties.is_empty() {
            None
        } else {
            Some(&self.text_properties)
        }
    }

    pub(in crate::buffer) fn into_insert_plan(
        self,
        anchor: TextPositionAnchor,
        marker_placement: InsertMarkerPlacement,
        marker_adjustment: InsertMarkerAdjustment,
    ) -> InsertTextPlan {
        let insertion = TextInsertion::at_anchor(anchor, self.new_extent());
        let edit = MeasuredInsertEdit::new(insertion, marker_placement, marker_adjustment);
        InsertTextPlan::from_measured_insert(self.bytes, self.text_properties, edit)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) struct MeasuredDeleteEdit {
    range: TextEditRange,
}

impl MeasuredDeleteEdit {
    pub(in crate::buffer) const fn new(range: TextEditRange) -> Self {
        Self { range }
    }

    pub(in crate::buffer) const fn range(self) -> TextEditRange {
        self.range
    }

    pub(in crate::buffer) const fn is_empty(self) -> bool {
        self.range.is_empty()
    }

    pub(in crate::buffer) const fn char_len(self) -> CharLen {
        self.range.char_len()
    }

    pub(in crate::buffer) fn state_after(
        self,
        state: BufferEditState,
        policy: DeleteSideEffectPolicy,
    ) -> BufferEditState {
        delete_state_after_edit(state, self.range, policy)
    }
}

/// Backend-neutral storage plan for deleting a measured range.
///
/// GNU deletion records both the deleted text and per-marker undo adjustments
/// before the storage mutation.  Keep those precomputed side inputs with the
/// measured delete so the executor does not pass independent range, text, and
/// marker-adjustment values through the pipeline.
#[derive(Clone, Debug)]
pub(in crate::buffer) struct DeleteTextPlan {
    edit: MeasuredDeleteEdit,
    /// The deleted text with its properties, built only when a caller wants
    /// it or undo will record it (see [`DeletionString`]).
    deleted_text: Option<LispString>,
    marker_adjustments: Vec<(Value, i64)>,
}

/// Whether a delete must produce its deleted text as a string: GNU
/// `del_range_2`'s `ret_string`.  `IfRecorded` builds it only for the undo
/// record (when undo is enabled); `Wanted` always, for the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) enum DeletionString {
    IfRecorded,
    Wanted,
}

impl DeleteTextPlan {
    pub(in crate::buffer) fn new(
        range: TextEditRange,
        deleted_text: Option<LispString>,
        marker_adjustments: Vec<(Value, i64)>,
    ) -> Self {
        Self {
            edit: MeasuredDeleteEdit::new(range),
            deleted_text,
            marker_adjustments,
        }
    }

    pub(in crate::buffer) const fn edit(&self) -> MeasuredDeleteEdit {
        self.edit
    }

    pub(in crate::buffer) const fn range(&self) -> TextEditRange {
        self.edit.range()
    }

    pub(in crate::buffer) fn deleted_text(&self) -> Option<&LispString> {
        self.deleted_text.as_ref()
    }

    pub(in crate::buffer) fn take_deleted_text(&mut self) -> Option<LispString> {
        self.deleted_text.take()
    }

    pub(in crate::buffer) fn marker_adjustments(&self) -> &[(Value, i64)] {
        &self.marker_adjustments
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) struct MeasuredReplaceEdit {
    replacement: TextReplacement,
}

impl MeasuredReplaceEdit {
    pub(in crate::buffer) const fn new(replacement: TextReplacement) -> Self {
        Self { replacement }
    }

    pub(in crate::buffer) const fn replacement(self) -> TextReplacement {
        self.replacement
    }

    pub(in crate::buffer) const fn is_empty(self) -> bool {
        self.replacement.old_range().is_empty() && self.replacement.new_extent().is_empty()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(in crate::buffer) const fn old_range(self) -> TextEditRange {
        self.replacement.old_range()
    }

    pub(in crate::buffer) const fn old_char_start(self) -> CharPos0 {
        self.replacement.old_range().char_start()
    }

    pub(in crate::buffer) const fn old_char_len(self) -> CharLen {
        self.replacement.old_char_len()
    }

    pub(in crate::buffer) const fn new_extent(self) -> TextExtent {
        self.replacement.new_extent()
    }

    pub(in crate::buffer) const fn new_char_len(self) -> CharLen {
        self.replacement.new_char_len()
    }

    pub(in crate::buffer) const fn changed_chars(self) -> CharLen {
        self.replacement.changed_chars()
    }

    pub(in crate::buffer) fn state_after(
        self,
        state: BufferEditState,
        policy: ReplaceSideEffectPolicy,
    ) -> BufferEditState {
        if policy.state_fields.update_state_fields() {
            replace_state_after_edit(state, self.replacement)
        } else {
            state
        }
    }
}

/// A same-byte-length text edit whose storage mutation span can differ from
/// the GNU-visible modified span.
///
/// GNU `subst-char-in-region` is the important case: it rewrites bytes across
/// the requested storage range, but `modify_text` starts at the first changed
/// character and runs through the original end.  `transpose-regions` uses the
/// same range for both.  Keeping both ranges typed avoids leaking raw
/// `changed_chars` counters through the edit pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) struct MeasuredSameLenEdit {
    storage_range: TextEditRange,
    modified_range: TextEditRange,
}

impl MeasuredSameLenEdit {
    pub(in crate::buffer) const fn new(
        storage_range: TextEditRange,
        modified_range: TextEditRange,
    ) -> Self {
        Self {
            storage_range,
            modified_range,
        }
    }

    pub(in crate::buffer) const fn covering(range: TextEditRange) -> Self {
        Self::new(range, range)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(in crate::buffer) const fn storage_range(self) -> TextEditRange {
        self.storage_range
    }

    pub(in crate::buffer) const fn modified_range(self) -> TextEditRange {
        self.modified_range
    }

    pub(in crate::buffer) const fn is_empty(self) -> bool {
        self.storage_range.is_empty() || self.modified_range.is_empty()
    }

    pub(in crate::buffer) const fn changed_chars(self) -> CharLen {
        self.modified_range.char_len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) enum SharedTextEditMetadata {
    Insert(MeasuredInsertEdit),
    Delete(MeasuredDeleteEdit),
    Replace(MeasuredReplaceEdit),
    SameLen {
        edit: MeasuredSameLenEdit,
        modified_state: SameLenModifiedStatePolicy,
    },
    Transposition {
        edit: MeasuredSameLenEdit,
        transposition: TextTransposition,
        modified_state: SameLenModifiedStatePolicy,
    },
}

impl SharedTextEditMetadata {
    pub(in crate::buffer) fn state_policy_for_shared_sibling(
        self,
        state_update: impl FnOnce() -> SharedBufferStateUpdate,
    ) -> SharedTextEditStatePolicy {
        match self {
            Self::Insert(_) | Self::Delete(_) | Self::Replace(_) | Self::Transposition { .. } => {
                SharedTextEditStatePolicy::StateFields(state_update())
            }
            Self::SameLen { .. } => SharedTextEditStatePolicy::NoStateFields,
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::buffer) struct SharedTextEditScope {
    edited_id: BufferId,
    buffer_ids: Vec<BufferId>,
}

impl SharedTextEditScope {
    pub(in crate::buffer) fn new(edited_id: BufferId, buffer_ids: Vec<BufferId>) -> Self {
        Self {
            edited_id,
            buffer_ids,
        }
    }

    pub(in crate::buffer) fn siblings(&self) -> impl Iterator<Item = BufferId> + '_ {
        self.buffer_ids
            .iter()
            .copied()
            .filter(|buffer_id| *buffer_id != self.edited_id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) enum SharedBufferStateUpdate {
    UpdateFields,
    RefreshFromStateMarkers,
}

impl SharedBufferStateUpdate {
    pub(in crate::buffer) const fn update_state_fields(self) -> bool {
        matches!(self, Self::UpdateFields)
    }

    pub(in crate::buffer) const fn needs_state_marker_refresh(self) -> bool {
        matches!(self, Self::RefreshFromStateMarkers)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) enum SharedTextEditStatePolicy {
    NoStateFields,
    StateFields(SharedBufferStateUpdate),
}

impl SharedTextEditStatePolicy {
    pub(in crate::buffer) const fn state_update(self) -> Option<SharedBufferStateUpdate> {
        match self {
            Self::NoStateFields => None,
            Self::StateFields(state_update) => Some(state_update),
        }
    }

    pub(in crate::buffer) const fn structural_state_update(self) -> SharedBufferStateUpdate {
        match self {
            Self::StateFields(state_update) => state_update,
            Self::NoStateFields => {
                panic!("shared structural edit requires state update policy")
            }
        }
    }
}

/// Backend-neutral plan for GNU `subst-char-in-region`.
///
/// GNU scans the buffer once to find each single-character replacement, records
/// undo per changed character, then rewrites the original storage range in
/// place because FROM and TO have the same Emacs-byte length.  Keeping the
/// replacement bytes together with the per-character changed ranges prevents
/// callers from recomputing those paired byte/char spans independently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::buffer) struct SameLenSubstitutionPlan {
    replacement_bytes: Vec<u8>,
    changed_ranges: Vec<TextEditRange>,
}

impl SameLenSubstitutionPlan {
    pub(in crate::buffer) fn new(
        range: TextEditRange,
        region_bytes: &[u8],
        multibyte: bool,
        from_code: u32,
        to_bytes: &[u8],
    ) -> Option<Self> {
        let mut replacement_bytes = Vec::with_capacity(region_bytes.len());
        let mut changed_ranges = Vec::new();
        if multibyte {
            Self::append_multibyte_substitutions(
                range,
                region_bytes,
                from_code,
                to_bytes,
                &mut replacement_bytes,
                &mut changed_ranges,
            );
        } else {
            Self::append_unibyte_substitutions(
                range,
                region_bytes,
                from_code,
                to_bytes,
                &mut replacement_bytes,
                &mut changed_ranges,
            )?;
        }

        if changed_ranges.is_empty() {
            None
        } else {
            Some(Self {
                replacement_bytes,
                changed_ranges,
            })
        }
    }

    fn append_multibyte_substitutions(
        range: TextEditRange,
        region_bytes: &[u8],
        from_code: u32,
        to_bytes: &[u8],
        replacement_bytes: &mut Vec<u8>,
        changed_ranges: &mut Vec<TextEditRange>,
    ) {
        let mut byte_offset = 0;
        let mut char_offset = 0;
        while byte_offset < region_bytes.len() {
            let (code, len) =
                crate::emacs_core::emacs_char::string_char(&region_bytes[byte_offset..]);
            let clen = len.max(1);
            if code == from_code {
                debug_assert_eq!(
                    clen,
                    to_bytes.len(),
                    "subst-char-in-region: matched char byte length ({}) must equal replacement length ({})",
                    clen,
                    to_bytes.len()
                );
                replacement_bytes.extend_from_slice(to_bytes);
                changed_ranges.push(range.subrange_from_start_offsets(
                    EmacsByteLen::new(byte_offset),
                    CharLen::new(char_offset),
                    TextExtent::new(CharLen::new(1), EmacsByteLen::new(clen)),
                ));
            } else {
                replacement_bytes.extend_from_slice(&region_bytes[byte_offset..byte_offset + clen]);
            }
            byte_offset += clen;
            char_offset += 1;
        }
    }

    fn append_unibyte_substitutions(
        range: TextEditRange,
        region_bytes: &[u8],
        from_code: u32,
        to_bytes: &[u8],
        replacement_bytes: &mut Vec<u8>,
        changed_ranges: &mut Vec<TextEditRange>,
    ) -> Option<()> {
        if from_code > 0xFF || to_bytes.len() != 1 {
            return None;
        }
        let from_byte = from_code as u8;
        for (index, &byte) in region_bytes.iter().enumerate() {
            if byte == from_byte {
                replacement_bytes.push(to_bytes[0]);
                changed_ranges.push(range.subrange_from_start_offsets(
                    EmacsByteLen::new(index),
                    CharLen::new(index),
                    TextExtent::new(CharLen::new(1), EmacsByteLen::new(1)),
                ));
            } else {
                replacement_bytes.push(byte);
            }
        }
        Some(())
    }

    pub(in crate::buffer) fn replacement_bytes(&self) -> &[u8] {
        &self.replacement_bytes
    }

    pub(in crate::buffer) fn changed_ranges(&self) -> &[TextEditRange] {
        &self.changed_ranges
    }

    pub(in crate::buffer) fn first_to_last_changed_range(&self) -> TextEditRange {
        let first = self
            .changed_ranges
            .first()
            .expect("substitution plan should contain at least one changed range");
        let last = self
            .changed_ranges
            .last()
            .expect("substitution plan should contain at least one changed range");
        TextEditRange::from_start_end(first.start_anchor(), last.end_anchor())
    }

    pub(in crate::buffer) fn replacement_for_range(
        &self,
        range: TextEditRange,
        multibyte: bool,
    ) -> TextReplacement {
        TextReplacement::new(
            range,
            TextExtent::from_emacs_bytes(&self.replacement_bytes, multibyte),
        )
    }
}

/// Backend-neutral storage plan for GNU `transpose-regions`.
///
/// GNU transposes bytes over the full `[start1, end2)` span without changing
/// the span's total size.  Undo records and text properties still have special
/// character-length cases, so this plan only owns the storage replacement and
/// the measured same-size edit descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::buffer) struct TranspositionStoragePlan {
    transposition: TextTransposition,
    replacement_bytes: Vec<u8>,
    replacement: TextReplacement,
    edit: MeasuredSameLenEdit,
}

impl TranspositionStoragePlan {
    pub(in crate::buffer) fn new(
        transposition: TextTransposition,
        first: &[u8],
        middle: &[u8],
        second: &[u8],
    ) -> Self {
        let span = transposition.span_edit_range();
        let mut replacement_bytes = Vec::with_capacity(span.byte_len().get());
        replacement_bytes.extend_from_slice(second);
        replacement_bytes.extend_from_slice(middle);
        replacement_bytes.extend_from_slice(first);
        debug_assert_eq!(
            replacement_bytes.len(),
            span.byte_len().get(),
            "transpose-regions storage replacement must preserve byte length"
        );
        let replacement = TextReplacement::new(span, span.extent());
        Self {
            transposition,
            replacement_bytes,
            replacement,
            edit: MeasuredSameLenEdit::covering(span),
        }
    }

    pub(in crate::buffer) const fn transposition(&self) -> TextTransposition {
        self.transposition
    }

    pub(in crate::buffer) fn replacement_bytes(&self) -> &[u8] {
        &self.replacement_bytes
    }

    pub(in crate::buffer) const fn replacement(&self) -> TextReplacement {
        self.replacement
    }

    pub(in crate::buffer) const fn edit(&self) -> MeasuredSameLenEdit {
        self.edit
    }
}

pub(in crate::buffer) struct SharedTextEditOutcome<T> {
    result: T,
    edit: Option<SharedTextEditMetadata>,
}

impl<T> SharedTextEditOutcome<T> {
    pub(in crate::buffer) const fn no_edit(result: T) -> Self {
        Self { result, edit: None }
    }

    pub(in crate::buffer) const fn edited(result: T, edit: SharedTextEditMetadata) -> Self {
        Self {
            result,
            edit: Some(edit),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) enum BufferStateFieldUpdatePolicy {
    Update,
    Skip,
}

impl BufferStateFieldUpdatePolicy {
    pub(in crate::buffer) const fn from_shared_update(
        state_update: SharedBufferStateUpdate,
    ) -> Self {
        if state_update.update_state_fields() {
            Self::Update
        } else {
            Self::Skip
        }
    }

    pub(in crate::buffer) const fn update_state_fields(self) -> bool {
        matches!(self, Self::Update)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) enum AccessibleStartUpdatePolicy {
    Preserve,
    ShiftAfterEdit,
}

impl AccessibleStartUpdatePolicy {
    pub(in crate::buffer) const fn shift_after_edit(self) -> bool {
        matches!(self, Self::ShiftAfterEdit)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) enum InsertPointUpdatePolicy {
    AdvanceAtInsertion,
    StayBeforeInsertion,
}

impl InsertPointUpdatePolicy {
    pub(in crate::buffer) const fn advance_point_at_insertion(self) -> bool {
        matches!(self, Self::AdvanceAtInsertion)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) enum SharedTextSideDataPolicy {
    AdjustInThisBuffer,
    AlreadyAdjustedInSharedText,
}

impl SharedTextSideDataPolicy {
    pub(in crate::buffer) const fn adjust_in_this_buffer(self) -> bool {
        matches!(self, Self::AdjustInThisBuffer)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) enum SameLenModifiedStatePolicy {
    RecordChange,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    PreserveUnmodifiedIfClean,
}

impl SameLenModifiedStatePolicy {
    pub(in crate::buffer) const fn preserve_unmodified_if_clean(self) -> bool {
        matches!(self, Self::PreserveUnmodifiedIfClean)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) struct InsertSideEffectPolicy {
    pub(in crate::buffer) state_fields: BufferStateFieldUpdatePolicy,
    pub(in crate::buffer) accessible_start: AccessibleStartUpdatePolicy,
    pub(in crate::buffer) point_at_insertion: InsertPointUpdatePolicy,
    pub(in crate::buffer) shared_side_data: SharedTextSideDataPolicy,
}

impl InsertSideEffectPolicy {
    pub(in crate::buffer) fn current_buffer() -> Self {
        Self {
            state_fields: BufferStateFieldUpdatePolicy::Update,
            accessible_start: AccessibleStartUpdatePolicy::Preserve,
            point_at_insertion: InsertPointUpdatePolicy::AdvanceAtInsertion,
            shared_side_data: SharedTextSideDataPolicy::AdjustInThisBuffer,
        }
    }

    pub(in crate::buffer) fn shared_buffer(state_update: SharedBufferStateUpdate) -> Self {
        Self {
            state_fields: BufferStateFieldUpdatePolicy::from_shared_update(state_update),
            accessible_start: AccessibleStartUpdatePolicy::ShiftAfterEdit,
            point_at_insertion: InsertPointUpdatePolicy::StayBeforeInsertion,
            shared_side_data: SharedTextSideDataPolicy::AlreadyAdjustedInSharedText,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) struct DeleteSideEffectPolicy {
    pub(in crate::buffer) state_fields: BufferStateFieldUpdatePolicy,
    pub(in crate::buffer) accessible_start: AccessibleStartUpdatePolicy,
    pub(in crate::buffer) shared_side_data: SharedTextSideDataPolicy,
}

impl DeleteSideEffectPolicy {
    pub(in crate::buffer) fn current_buffer() -> Self {
        Self {
            state_fields: BufferStateFieldUpdatePolicy::Update,
            accessible_start: AccessibleStartUpdatePolicy::Preserve,
            shared_side_data: SharedTextSideDataPolicy::AdjustInThisBuffer,
        }
    }

    pub(in crate::buffer) fn shared_buffer(state_update: SharedBufferStateUpdate) -> Self {
        Self {
            state_fields: BufferStateFieldUpdatePolicy::from_shared_update(state_update),
            accessible_start: AccessibleStartUpdatePolicy::ShiftAfterEdit,
            shared_side_data: SharedTextSideDataPolicy::AlreadyAdjustedInSharedText,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::buffer) struct ReplaceSideEffectPolicy {
    pub(in crate::buffer) state_fields: BufferStateFieldUpdatePolicy,
    pub(in crate::buffer) shared_side_data: SharedTextSideDataPolicy,
}

impl ReplaceSideEffectPolicy {
    pub(in crate::buffer) fn current_buffer() -> Self {
        Self {
            state_fields: BufferStateFieldUpdatePolicy::Update,
            shared_side_data: SharedTextSideDataPolicy::AdjustInThisBuffer,
        }
    }

    pub(in crate::buffer) fn shared_buffer(state_update: SharedBufferStateUpdate) -> Self {
        Self {
            state_fields: BufferStateFieldUpdatePolicy::from_shared_update(state_update),
            shared_side_data: SharedTextSideDataPolicy::AlreadyAdjustedInSharedText,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(
        pt_byte: usize,
        pt: usize,
        begv_byte: usize,
        begv: usize,
        zv_byte: usize,
        zv: usize,
    ) -> BufferEditState {
        BufferEditState::new(
            TextPositionAnchor::from_usize(pt, pt_byte),
            TextPositionAnchor::from_usize(begv, begv_byte),
            TextPositionAnchor::from_usize(zv, zv_byte),
        )
    }

    fn replace_state(old: BufferEditState) -> BufferEditState {
        replace_state_after_edit(
            old,
            TextReplacement::new(
                TextEditRange::from_usize(20, 36, 10, 18),
                crate::buffer::TextExtent::from_usize(3, 5),
            ),
        )
    }

    fn same_len_edit() -> MeasuredSameLenEdit {
        MeasuredSameLenEdit::new(
            TextEditRange::from_usize(0, 10, 0, 10),
            TextEditRange::from_usize(3, 10, 3, 10),
        )
    }

    fn insertion() -> TextInsertion {
        TextInsertion::from_usize(20, 10, 3, 5)
    }

    fn deleted_range() -> TextEditRange {
        TextEditRange::from_usize(20, 36, 10, 18)
    }

    fn measured_insert_edit() -> MeasuredInsertEdit {
        MeasuredInsertEdit::by_insertion_type(insertion(), InsertMarkerPlacement::AfterMarkers)
    }

    fn measured_delete_edit() -> MeasuredDeleteEdit {
        MeasuredDeleteEdit::new(deleted_range())
    }

    fn measured_replace_edit() -> MeasuredReplaceEdit {
        MeasuredReplaceEdit::new(TextReplacement::new(
            deleted_range(),
            TextExtent::from_usize(3, 5),
        ))
    }

    fn state_policy_for_shared_sibling(edit: SharedTextEditMetadata) -> SharedTextEditStatePolicy {
        edit.state_policy_for_shared_sibling(|| SharedBufferStateUpdate::RefreshFromStateMarkers)
    }

    #[test]
    fn shared_edit_metadata_derives_sibling_state_policy() {
        assert_eq!(
            state_policy_for_shared_sibling(SharedTextEditMetadata::Insert(measured_insert_edit())),
            SharedTextEditStatePolicy::StateFields(
                SharedBufferStateUpdate::RefreshFromStateMarkers
            )
        );
        assert_eq!(
            state_policy_for_shared_sibling(SharedTextEditMetadata::Delete(measured_delete_edit())),
            SharedTextEditStatePolicy::StateFields(
                SharedBufferStateUpdate::RefreshFromStateMarkers
            )
        );
        assert_eq!(
            state_policy_for_shared_sibling(SharedTextEditMetadata::Replace(
                measured_replace_edit()
            )),
            SharedTextEditStatePolicy::StateFields(
                SharedBufferStateUpdate::RefreshFromStateMarkers
            )
        );
        assert_eq!(
            state_policy_for_shared_sibling(SharedTextEditMetadata::Transposition {
                edit: same_len_edit(),
                transposition: TextTransposition::from_usize(2, 5, 1, 3, 8, 10, 5, 7),
                modified_state: SameLenModifiedStatePolicy::PreserveUnmodifiedIfClean,
            }),
            SharedTextEditStatePolicy::StateFields(
                SharedBufferStateUpdate::RefreshFromStateMarkers
            )
        );
        assert_eq!(
            state_policy_for_shared_sibling(SharedTextEditMetadata::SameLen {
                edit: same_len_edit(),
                modified_state: SameLenModifiedStatePolicy::PreserveUnmodifiedIfClean,
            }),
            SharedTextEditStatePolicy::NoStateFields
        );
    }

    #[test]
    fn insert_state_current_buffer_advances_point_at_insert_and_zv() {
        assert_eq!(
            insert_state_after_edit(
                state(20, 10, 0, 0, 60, 42),
                insertion(),
                InsertSideEffectPolicy::current_buffer(),
            ),
            state(25, 13, 0, 0, 65, 45)
        );
    }

    #[test]
    fn insert_state_shared_buffer_keeps_point_at_insert_and_shifts_begv_after_insert() {
        assert_eq!(
            insert_state_after_edit(
                state(20, 10, 28, 14, 60, 42),
                insertion(),
                InsertSideEffectPolicy::shared_buffer(SharedBufferStateUpdate::UpdateFields),
            ),
            state(20, 10, 33, 17, 65, 45)
        );
    }

    #[test]
    fn insert_state_shifts_zv_at_insert_position() {
        assert_eq!(
            insert_state_after_edit(
                state(0, 0, 0, 0, 20, 10),
                insertion(),
                InsertSideEffectPolicy::shared_buffer(SharedBufferStateUpdate::UpdateFields),
            ),
            state(0, 0, 0, 0, 25, 13)
        );
    }

    #[test]
    fn delete_state_current_buffer_maps_point_inside_range_to_deleted_start() {
        assert_eq!(
            delete_state_after_edit(
                state(28, 14, 0, 0, 60, 42),
                deleted_range(),
                DeleteSideEffectPolicy::current_buffer(),
            ),
            state(20, 10, 0, 0, 44, 34)
        );
    }

    #[test]
    fn delete_state_keeps_point_at_deleted_start() {
        assert_eq!(
            delete_state_after_edit(
                state(20, 10, 0, 0, 60, 42),
                deleted_range(),
                DeleteSideEffectPolicy::current_buffer(),
            ),
            state(20, 10, 0, 0, 44, 34)
        );
    }

    #[test]
    fn delete_state_shared_buffer_shifts_point_begv_and_zv() {
        assert_eq!(
            delete_state_after_edit(
                state(44, 24, 28, 14, 60, 42),
                deleted_range(),
                DeleteSideEffectPolicy::shared_buffer(SharedBufferStateUpdate::UpdateFields),
            ),
            state(28, 16, 20, 10, 44, 34)
        );
    }

    #[test]
    fn insert_and_delete_state_skip_update_when_policy_disables_state_fields() {
        let original = state(44, 24, 28, 14, 60, 42);

        assert_eq!(
            insert_state_after_edit(
                original,
                insertion(),
                InsertSideEffectPolicy::shared_buffer(
                    SharedBufferStateUpdate::RefreshFromStateMarkers,
                ),
            ),
            original
        );
        assert_eq!(
            delete_state_after_edit(
                original,
                deleted_range(),
                DeleteSideEffectPolicy::shared_buffer(
                    SharedBufferStateUpdate::RefreshFromStateMarkers,
                ),
            ),
            original
        );
    }

    #[test]
    fn replace_state_maps_point_inside_deleted_range_to_replacement_end() {
        assert_eq!(
            replace_state(state(28, 14, 0, 0, 60, 42)),
            state(25, 13, 0, 0, 49, 37)
        );
    }

    #[test]
    fn replace_state_keeps_point_at_deleted_start() {
        assert_eq!(
            replace_state(state(20, 10, 0, 0, 60, 42)),
            state(20, 10, 0, 0, 49, 37)
        );
    }

    #[test]
    fn replace_state_maps_point_at_deleted_end_to_replacement_end() {
        assert_eq!(
            replace_state(state(36, 18, 0, 0, 60, 42)),
            state(25, 13, 0, 0, 49, 37)
        );
    }

    #[test]
    fn replace_state_shifts_point_after_deleted_range_by_extent_delta() {
        assert_eq!(
            replace_state(state(44, 24, 0, 0, 60, 42)),
            state(33, 19, 0, 0, 49, 37)
        );
    }

    #[test]
    fn replace_state_clamps_begv_inside_deleted_range_to_deleted_start() {
        assert_eq!(
            replace_state(state(0, 0, 28, 14, 60, 42)),
            state(0, 0, 20, 10, 49, 37)
        );
    }

    #[test]
    fn replace_state_maps_begv_at_deleted_end_to_deleted_start() {
        assert_eq!(
            replace_state(state(0, 0, 36, 18, 60, 42)),
            state(0, 0, 20, 10, 49, 37)
        );
    }

    #[test]
    fn replace_state_maps_zv_inside_deleted_range_to_replacement_end() {
        assert_eq!(
            replace_state(state(0, 0, 0, 0, 28, 14)),
            state(0, 0, 0, 0, 25, 13)
        );
    }

    #[test]
    fn modification_tick_delta_is_logarithmic_and_never_zero() {
        assert_eq!(modification_tick_delta(CharLen::new(0)), 1);
        assert_eq!(modification_tick_delta(CharLen::new(1)), 1);
        assert_eq!(modification_tick_delta(CharLen::new(2)), 2);
        assert_eq!(modification_tick_delta(CharLen::new(3)), 2);
        assert_eq!(modification_tick_delta(CharLen::new(4)), 3);
        assert_eq!(modification_tick_delta(CharLen::new(8)), 4);
    }

    #[test]
    fn same_len_edit_keeps_storage_and_modified_ranges_separate() {
        let edit = same_len_edit();

        assert_eq!(
            edit.storage_range(),
            TextEditRange::from_usize(0, 10, 0, 10)
        );
        assert_eq!(
            edit.modified_range(),
            TextEditRange::from_usize(3, 10, 3, 10)
        );
        assert_eq!(edit.changed_chars(), CharLen::new(7));
    }

    #[test]
    fn same_len_substitution_plan_records_per_character_multibyte_ranges() {
        let range = TextEditRange::from_usize(0, "a日本日".len(), 0, 4);
        let plan = SameLenSubstitutionPlan::new(
            range,
            "a日本日".as_bytes(),
            true,
            '日' as u32,
            "本".as_bytes(),
        )
        .expect("matching chars should produce a substitution plan");

        assert_eq!(plan.replacement_bytes(), "a本本本".as_bytes());
        assert_eq!(
            plan.changed_ranges(),
            &[
                TextEditRange::from_usize(1, 4, 1, 2),
                TextEditRange::from_usize(7, 10, 3, 4),
            ]
        );
        assert_eq!(
            plan.first_to_last_changed_range(),
            TextEditRange::from_usize(1, 10, 1, 4)
        );
        assert_eq!(
            plan.replacement_for_range(range, true),
            TextReplacement::new(range, TextExtent::from_usize(4, "a本本本".len()))
        );
    }

    #[test]
    fn same_len_substitution_plan_records_unibyte_ranges_and_rejects_non_bytes() {
        let range = TextEditRange::from_usize(20, 25, 10, 15);
        let plan = SameLenSubstitutionPlan::new(range, b"ababa", false, b'a' as u32, b"z")
            .expect("matching unibyte chars should produce a substitution plan");

        assert_eq!(plan.replacement_bytes(), b"zbzbz");
        assert_eq!(
            plan.changed_ranges(),
            &[
                TextEditRange::from_usize(20, 21, 10, 11),
                TextEditRange::from_usize(22, 23, 12, 13),
                TextEditRange::from_usize(24, 25, 14, 15),
            ]
        );
        assert_eq!(
            plan.first_to_last_changed_range(),
            TextEditRange::from_usize(20, 25, 10, 15)
        );
        assert!(SameLenSubstitutionPlan::new(range, b"ababa", false, 0x100, b"z").is_none());
        assert!(SameLenSubstitutionPlan::new(range, b"ababa", false, b'a' as u32, b"zz").is_none());
    }

    #[test]
    fn same_len_substitution_plan_returns_none_without_matches() {
        let range = TextEditRange::from_usize(0, 5, 0, 5);

        assert!(SameLenSubstitutionPlan::new(range, b"abcde", true, b'z' as u32, b"q").is_none());
    }

    #[test]
    fn transposition_storage_plan_swaps_outer_regions_over_full_span() {
        let transposition = TextTransposition::from_usize(2, 5, 1, 3, 8, 10, 5, 7);
        let plan = TranspositionStoragePlan::new(transposition, b"abc", b"XYZ", b"de");
        let span = TextEditRange::from_usize(2, 10, 1, 7);

        assert_eq!(plan.replacement_bytes(), b"deXYZabc");
        assert_eq!(
            plan.replacement(),
            TextReplacement::new(span, span.extent())
        );
        assert_eq!(plan.edit(), MeasuredSameLenEdit::covering(span));
    }
}
