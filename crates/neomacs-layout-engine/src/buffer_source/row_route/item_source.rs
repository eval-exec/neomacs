//! The `DisplayItemSource` a planned row is rendered through: it replays the
//! plan's segments as display items, which is what makes the routed row the
//! SAME renderer path as every other item source.

use super::*;

/// One text segment of a routed row: `[start, end)` rendered with `face`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PlainRowItemSegment {
    pub(crate) start: CharPos0,
    pub(crate) end: CharPos0,
    pub(crate) face: RenderFaceRef,
}

/// A `DisplayItemSource` over one classified plain-ASCII buffer row. Produces
/// exactly the items `BufferTextSourceCursor` would for the same row — one
/// plain `TextRun` per face segment (one for the whole line when properties
/// are constant), then (when the row break is included) the explicit-newline
/// `RowBreak` — mirroring GNU `next_element_from_buffer` yielding the line's
/// characters, re-segmented at each `compute_stop_pos` stop, and then the
/// newline element.
pub(crate) struct BufferPlainItemSource {
    items: std::collections::VecDeque<DisplayItem>,
}

impl BufferPlainItemSource {
    /// Source over `[start, line_end)` text plus the newline row break at
    /// `line_end` — the full row, as the shadow renderer consumes it. The
    /// single-segment shape only the shadow-equivalence tests need; execution
    /// always builds a source through
    /// [`with_row_break_segments`](Self::with_row_break_segments).
    #[cfg(test)]
    pub(crate) fn with_row_break<B: LayoutBufferView + ?Sized>(
        buffer_id: BufferId,
        buffer: &B,
        start: CharPos0,
        line_end: CharPos0,
        face: RenderFaceRef,
    ) -> Self {
        Self::from_segments(
            buffer_id,
            buffer,
            &[PlainRowItemSegment {
                start,
                end: line_end,
                face,
            }],
            Some((line_end, face)),
        )
    }

    /// Source over the row's face segments plus the newline row break at
    /// `line_end` (the newline's OWN char position — with a trailing elision
    /// the last visible segment ends before it), the break carrying the face
    /// resolved AT the newline (a face span covering the newline rides onto
    /// the appended newline space through the line-end plan, mirroring the
    /// buffer pipeline's row-break face).
    pub(crate) fn with_row_break_segments<B: LayoutBufferView + ?Sized>(
        buffer_id: BufferId,
        buffer: &B,
        segments: &[PlainRowItemSegment],
        line_end: CharPos0,
        row_break_face: RenderFaceRef,
    ) -> Self {
        Self::from_segments(
            buffer_id,
            buffer,
            segments,
            Some((line_end, row_break_face)),
        )
    }

    /// Source over the line text only; the buffer pipeline's own line-break
    /// lifecycle (line-end plan, appended newline space, row transition)
    /// consumes the newline. Used by the routed production render, one
    /// segment per call so each renders under its own active face.
    pub(crate) fn text_only<B: LayoutBufferView + ?Sized>(
        buffer_id: BufferId,
        buffer: &B,
        start: CharPos0,
        line_end: CharPos0,
        face: RenderFaceRef,
    ) -> Self {
        Self::from_segments(
            buffer_id,
            buffer,
            &[PlainRowItemSegment {
                start,
                end: line_end,
                face,
            }],
            None,
        )
    }

    fn from_segments<B: LayoutBufferView + ?Sized>(
        buffer_id: BufferId,
        buffer: &B,
        segments: &[PlainRowItemSegment],
        row_break: Option<(CharPos0, RenderFaceRef)>,
    ) -> Self {
        let byte_at = |pos: CharPos0| buffer.layout_char_pos_to_emacs_byte_pos(pos);
        let span = |from: CharPos0, to: CharPos0| {
            SourceSpan::new(
                DisplaySourcePosition::buffer(buffer_id, from, byte_at(from)),
                DisplaySourcePosition::buffer(buffer_id, to, byte_at(to)),
            )
        };

        let mut items = std::collections::VecDeque::with_capacity(segments.len() + 1);
        for segment in segments {
            if segment.end <= segment.start {
                continue;
            }
            let mut bytes = Vec::new();
            buffer.layout_copy_emacs_byte_range_to(
                EmacsByteRange::new(byte_at(segment.start), byte_at(segment.end)),
                &mut bytes,
            );
            let mut text = String::with_capacity(bytes.len());
            let mut offset = 0usize;
            while offset < bytes.len() {
                let (ch, len) = decode_utf8(&bytes[offset..]);
                debug_assert!(
                    len > 0 && ch.len_utf8() == len,
                    "BufferPlainItemSource requires well-formed UTF-8 row text"
                );
                if len == 0 {
                    break;
                }
                debug_assert!(
                    classify_routed_row_char(ch).is_some() || routed_composable_extender(ch),
                    "BufferPlainItemSource requires a classified routable row (got {ch:?})"
                );
                text.push(ch);
                offset += len;
            }
            items.push_back(
                DisplayItem::new(
                    span(segment.start, segment.end),
                    segment.face,
                    DisplayItemKind::TextRun(DisplayTextRun::independent(text)),
                )
                .with_layout(DisplayItemLayout::default())
                .with_pointer_appearance(None)
                // The routed producer is admitted only after every realized
                // segment is proven box-free.  Make that invariant explicit
                // instead of inheriting DisplayItem's closed synthetic-run
                // default; boxed rows use the canonical source cursor, which
                // owns GNU's source-neighbor topology calculation.
                .with_box_vertical_edges(neomacs_display_protocol::face::BoxVerticalEdges::Neither),
            );
        }

        if let Some((line_end, break_face)) = row_break {
            // Mirrors `BufferTextSourceCursor::next_text_item_with_layout`:
            // the newline's row break carries the line-height policy resolved
            // from the (absent, for a classified row) `line-height` property.
            let row_break = DisplayRowBreak::explicit_newline()
                .with_line_height(DisplayLineHeightPolicy::from_property(None));
            items.push_back(
                DisplayItem::new(
                    span(line_end, line_end.add_len(CharLen::new(1))),
                    break_face,
                    DisplayItemKind::RowBreak(row_break),
                )
                .with_layout(DisplayItemLayout::default())
                .with_pointer_appearance(None)
                .with_box_vertical_edges(neomacs_display_protocol::face::BoxVerticalEdges::Neither),
            );
        }

        Self { items }
    }

    /// The next `TextRun` item without consuming the source (the routed
    /// production render measures the run before committing to the route).
    pub(crate) fn text_item(&self) -> Option<&DisplayItem> {
        self.items
            .front()
            .filter(|item| matches!(item.kind, DisplayItemKind::TextRun(_)))
    }
}

impl crate::display_source::DisplayItemSource for BufferPlainItemSource {
    fn next_item(
        &mut self,
        _context: &mut crate::display_source::DisplaySourceContext<'_>,
    ) -> Option<DisplayItem> {
        self.items.pop_front()
    }
}
