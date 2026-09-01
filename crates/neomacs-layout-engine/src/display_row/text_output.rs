use crate::coords::layout_i64_char_pos_to_lisp_char_pos;
use crate::display_item::DisplaySourcePosition;
use crate::display_row::builder::{DisplayRowGlyphSlot, DisplayRowPosition};
use neovm_core::buffer::LispCharPos1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextRowOutput {
    row: usize,
    row_y: f32,
    glyph_y: f32,
    height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextOutputSpan {
    buffer_pos: LispCharPos1,
    row: usize,
    row_y: f32,
    glyph_y: f32,
    height: f32,
    start: DisplayRowPosition,
    end: DisplayRowPosition,
}

impl TextRowOutput {
    pub(crate) fn new(row: usize, row_y: f32, glyph_y: f32, height: f32) -> Self {
        Self {
            row,
            row_y,
            glyph_y,
            height,
        }
    }

    pub(crate) fn row(self) -> usize {
        self.row
    }

    pub(crate) fn row_y(self) -> f32 {
        self.row_y
    }

    #[cfg(test)]
    pub(crate) fn glyph_y(self) -> f32 {
        self.glyph_y
    }

    #[cfg(test)]
    pub(crate) fn height(self) -> f32 {
        self.height
    }

    fn span_for_buffer_slot(self, slot: &DisplayRowGlyphSlot) -> Option<TextOutputSpan> {
        let DisplaySourcePosition::Buffer { char_pos, .. } = slot.source() else {
            return None;
        };
        Some(TextOutputSpan::new(
            layout_i64_char_pos_to_lisp_char_pos(char_pos.get() as i64),
            self.row,
            self.row_y,
            self.glyph_y,
            self.height,
            slot.start_position(),
            slot.end_position(),
        ))
    }

    pub(crate) fn spans_for_source_slots(
        self,
        slots: &[DisplayRowGlyphSlot],
    ) -> Vec<TextOutputSpan> {
        let mut spans: Vec<TextOutputSpan> = Vec::new();
        for slot in slots {
            let Some(span) = self.span_for_buffer_slot(slot) else {
                continue;
            };
            if let Some(pending) = spans.last_mut()
                && pending.can_merge(span)
            {
                pending.merge(span);
                continue;
            }
            spans.push(span);
        }
        spans
    }
}

impl TextOutputSpan {
    pub(crate) fn new(
        buffer_pos: LispCharPos1,
        row: usize,
        row_y: f32,
        glyph_y: f32,
        height: f32,
        start: DisplayRowPosition,
        end: DisplayRowPosition,
    ) -> Self {
        Self {
            buffer_pos,
            row,
            row_y,
            glyph_y,
            height,
            start,
            end,
        }
    }

    pub(crate) fn buffer_pos(self) -> LispCharPos1 {
        self.buffer_pos
    }

    pub(crate) fn row(self) -> usize {
        self.row
    }

    pub(crate) fn row_y(self) -> f32 {
        self.row_y
    }

    pub(crate) fn glyph_y(self) -> f32 {
        self.glyph_y
    }

    pub(crate) fn height(self) -> f32 {
        self.height
    }

    pub(crate) fn start(self) -> DisplayRowPosition {
        self.start
    }

    pub(crate) fn end(self) -> DisplayRowPosition {
        self.end
    }

    fn can_merge(self, next: Self) -> bool {
        self.buffer_pos() == next.buffer_pos()
            && self.row() == next.row()
            && self.row_y() == next.row_y()
            && self.glyph_y() == next.glyph_y()
            && self.height() == next.height()
            && self.end() == next.start()
    }

    fn merge(&mut self, next: Self) {
        self.end = next.end();
    }
}
