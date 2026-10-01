//! A short-lived, read-only capture view. Borrowing buffer and symbol state
//! prevents mutation throughout acquisition without copying the whole buffer
//! or enumerating every category/overlay. It must never cross a worker boundary.

use super::buffer_snapshot::{
    capture_automatic_composition_spans, capture_string_composition_rules, resolve_layout_vars,
};
use super::{LayoutBufferView, LayoutVar};
use neovm_core::buffer::{
    Buffer, CharPos0, CharRange, EmacsByteLen, EmacsBytePos, EmacsByteRange, overlay::OverlayList,
};
use neovm_core::emacs_core::{Value, symbol::Obarray};

pub(crate) struct BorrowedLayoutBuffer<'a> {
    buffer: &'a Buffer,
    obarray: &'a Obarray,
    target: crate::display_property::DisplayPropertyTarget,
    vars: [Option<Value>; <LayoutVar as strum::EnumCount>::COUNT],
    spans: Vec<CharRange>,
    string_rules: Option<neovm_core::emacs_core::composite::AutomaticCompositionRules>,
}

impl<'a> BorrowedLayoutBuffer<'a> {
    pub(crate) fn for_window(
        buffer: &'a Buffer,
        obarray: &'a Obarray,
        start: CharPos0,
        max_chars: usize,
        target: crate::display_property::DisplayPropertyTarget,
    ) -> Self {
        let vars = resolve_layout_vars(buffer, Some(obarray));
        let spans = capture_automatic_composition_spans(
            buffer,
            obarray,
            &vars,
            Some((start.get(), max_chars)),
        );
        Self {
            buffer,
            obarray,
            target,
            vars,
            spans,
            string_rules: capture_string_composition_rules(buffer, obarray),
        }
    }
}

impl LayoutBufferView for BorrowedLayoutBuffer<'_> {
    fn layout_is_multibyte(&self) -> bool {
        self.buffer.get_multibyte()
    }

    fn layout_buffer_local_value(&self, var: LayoutVar) -> Option<Value> {
        self.vars[var as usize]
    }

    fn layout_point_min_emacs_byte_pos(&self) -> EmacsBytePos {
        self.buffer.point_min_emacs_byte_pos()
    }

    fn layout_point_max_emacs_byte_pos(&self) -> EmacsBytePos {
        self.buffer.point_max_emacs_byte_pos()
    }

    fn layout_point_max_char_pos(&self) -> CharPos0 {
        self.buffer.point_max_char_pos()
    }

    fn layout_total_emacs_byte_len(&self) -> EmacsByteLen {
        self.buffer.total_emacs_byte_len()
    }

    fn layout_char_pos_to_emacs_byte_pos(&self, charpos: CharPos0) -> EmacsBytePos {
        self.buffer.char_pos_to_emacs_byte_pos_clamped(charpos)
    }

    fn layout_emacs_byte_pos_to_char_pos(&self, bytepos: EmacsBytePos) -> CharPos0 {
        self.buffer.emacs_byte_pos_to_char_pos_clamped(bytepos)
    }

    fn layout_copy_emacs_byte_range_to(&self, range: EmacsByteRange, out: &mut Vec<u8>) {
        self.buffer.copy_emacs_byte_range_to(range, out);
    }

    fn layout_try_for_each_emacs_byte_range_chunk<E>(
        &self,
        range: EmacsByteRange,
        f: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        self.buffer.try_for_each_emacs_byte_range_chunk(range, f)
    }

    fn layout_emacs_byte_at_pos(&self, pos: EmacsBytePos) -> Option<u8> {
        self.buffer.emacs_byte_at_pos(pos)
    }

    fn layout_indexed_newline_count(&self, range: EmacsByteRange) -> Option<usize> {
        self.buffer.indexed_newline_count(range)
    }

    fn layout_text_prop_at_emacs_byte_pos(&self, pos: EmacsBytePos, name: Value) -> Option<Value> {
        self.buffer
            .text_props_get_property_at_emacs_byte_pos(pos, name)
    }

    fn layout_next_text_prop_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.buffer.text_props_next_change_after_emacs_byte_pos(pos)
    }

    fn layout_next_single_text_prop_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<EmacsBytePos> {
        self.buffer
            .text_props_next_single_change_after_emacs_byte_pos(pos, name)
    }

    fn layout_next_single_text_prop_change_after_emacs_byte_pos_bounded(
        &self,
        pos: EmacsBytePos,
        name: Value,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.buffer
            .text_props_next_single_change_after_emacs_byte_pos_bounded(pos, name, limit)
    }

    fn layout_previous_single_text_prop_change_before_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<EmacsBytePos> {
        self.buffer
            .text_props_previous_single_change_before_emacs_byte_pos(pos, name)
    }

    fn layout_overlays(&self) -> &OverlayList {
        self.buffer.overlays()
    }

    fn layout_display_target(&self) -> crate::display_property::DisplayPropertyTarget {
        self.target
    }
    fn layout_string_composition_rules(
        &self,
    ) -> Option<neovm_core::emacs_core::composite::AutomaticCompositionRules> {
        self.string_rules
    }
    fn layout_category_symbol_property(&self, category: Value, property: Value) -> Option<Value> {
        let id = category.as_symbol_id()?;
        neovm_core::emacs_core::symbol::SymbolPropertyRevision::observe(id);
        neovm_core::emacs_core::plist::plist_get(self.obarray.symbol_plist_id(id), &property)
    }
    fn layout_automatic_composition_starting_at(&self, pos: CharPos0) -> Option<CharRange> {
        self.spans.iter().copied().find(|span| span.start() == pos)
    }
    fn layout_next_automatic_composition_start(
        &self,
        pos: CharPos0,
        limit: CharPos0,
    ) -> Option<CharPos0> {
        // A span starting AT `pos` is reported, matching the snapshot view's
        // partition (`start() < pos`) — GNU's `composition_compute_stop_pos`
        // likewise reports a stop AT the position. The row router's
        // conservative `ScanCompose` refusal depends on the at-pos report
        // (a row may not route when a composition opens on its first char);
        // the pipeline's run scan never observes it, because the producer's
        // item arm emits an at-pos span first
        // (`layout_automatic_composition_starting_at`).
        self.spans
            .iter()
            .map(|span| span.start())
            .find(|start| *start >= pos && *start < limit)
    }
}
