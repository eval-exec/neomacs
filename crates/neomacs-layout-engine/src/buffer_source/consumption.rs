//! Buffer source typed item consumption.

use crate::buffer_source::text_source::{
    BufferOverlayStringsItem, BufferTextCursorItem, BufferTextDisplayReplacementMode,
    BufferTextSourceCursor,
};
use crate::display_item::{
    BufferDisplayPropertyReplacementItem, DisplayItem, DisplaySourcePosition,
};
use crate::display_source::{
    DisplaySourceContext, DisplaySourceItem, DisplaySourceStepItem, DisplaySourceTextPosition,
};
use crate::neovm_bridge::LayoutBufferView;
use neovm_core::buffer::CharPos0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BufferSourceConsumedItem {
    Renderable(DisplaySourceStepItem),
    DisplayPropertyReplacement(BufferDisplayPropertyReplacementItem),
    /// Overlay strings anchored at the current position, collected and ordered
    /// by the producer. Consuming one does NOT advance the walk position
    /// (insertion semantics), so the buffer character at the anchor follows.
    OverlayStrings(BufferOverlayStringsItem),
}

impl BufferSourceConsumedItem {
    #[cfg(test)]
    pub(crate) fn into_renderable(self) -> Option<DisplaySourceStepItem> {
        match self {
            Self::Renderable(item) => Some(item),
            Self::DisplayPropertyReplacement(_) | Self::OverlayStrings(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BufferSourceConsumptionState {
    text_start_byte: usize,
}

impl BufferSourceConsumptionState {
    pub(crate) fn new(text_start_byte: usize) -> Self {
        Self { text_start_byte }
    }

    fn prepare_render_source_item(
        &mut self,
        source_item: DisplaySourceItem,
    ) -> Option<DisplaySourceStepItem> {
        DisplaySourceStepItem::new(source_item, self.text_start_byte)
    }

    fn expected_source_pos(position: DisplaySourceTextPosition) -> CharPos0 {
        CharPos0::new(position.charpos().max(0) as usize)
    }

    fn align_display_item(
        &self,
        position: DisplaySourceTextPosition,
        source_char: Option<char>,
        item: DisplayItem,
    ) -> Option<DisplaySourceItem> {
        let DisplaySourcePosition::Buffer { byte_pos, .. } = item.span.start else {
            tracing::error!(
                "BufferSourceConsumptionState: source cursor yielded a non-buffer-span item; \
                 a display property escaped the render_next_step checkpoints"
            );
            return None;
        };
        let start_byte_idx = byte_pos.get().checked_sub(self.text_start_byte)?;
        if start_byte_idx != position.byte_idx() {
            tracing::error!(
                "BufferSourceConsumptionState: source cursor byte position {} did not match \
                 buffer walk byte index {}",
                start_byte_idx,
                position.byte_idx()
            );
            return None;
        }
        let DisplaySourcePosition::Buffer { char_pos, .. } = item.span.start else {
            unreachable!("buffer byte position match implies buffer source position");
        };
        let start_charpos = char_pos.get() as i64;
        if start_charpos != position.charpos() {
            tracing::error!(
                "BufferSourceConsumptionState: source cursor char position {} did not match \
                 buffer walk char position {}",
                start_charpos,
                position.charpos()
            );
            return None;
        }
        Some(DisplaySourceItem::new(
            item,
            start_byte_idx,
            start_charpos,
            source_char,
        ))
    }

    fn replacement_matches(
        &self,
        position: DisplaySourceTextPosition,
        item: &BufferDisplayPropertyReplacementItem,
    ) -> Option<bool> {
        let anchor = item.source_anchor(self.text_start_byte)?;
        Some(anchor.matches(position.byte_idx(), position.charpos()))
    }

    fn read_source_cursor<B: LayoutBufferView + ?Sized>(
        &self,
        source: &mut BufferTextSourceCursor<'_, B>,
        context: &mut DisplaySourceContext<'_>,
        position: DisplaySourceTextPosition,
        replacement_mode: BufferTextDisplayReplacementMode,
    ) -> Option<(BufferTextCursorItem, Option<char>)> {
        let expected_source_pos = Self::expected_source_pos(position);
        if source.current_char_pos() != expected_source_pos {
            source.reset_to(expected_source_pos);
        }

        let source_char = source.char_at(expected_source_pos);
        let item = source.next_cursor_item(context, replacement_mode)?;
        if let BufferTextCursorItem::DisplayPropertyReplacement(replacement) = &item
            && !self.replacement_matches(position, replacement)?
        {
            tracing::error!(
                "BufferSourceConsumptionState: display replacement did not match \
                         buffer walk byte {} charpos {}",
                position.byte_idx(),
                position.charpos()
            );
            return None;
        }
        Some((item, source_char))
    }

    #[cfg(test)]
    pub(crate) fn next_display_item_from_source<B: LayoutBufferView + ?Sized>(
        &mut self,
        source: &mut BufferTextSourceCursor<'_, B>,
        context: &mut DisplaySourceContext<'_>,
        position: &mut DisplaySourceTextPosition,
    ) -> Option<DisplaySourceItem> {
        let item = self.next_item_from_source(source, context, position)?;
        self.consume_aligned_display_item(item, position)
    }

    #[cfg(test)]
    pub(crate) fn next_item_from_source<B: LayoutBufferView + ?Sized>(
        &mut self,
        source: &mut BufferTextSourceCursor<'_, B>,
        context: &mut DisplaySourceContext<'_>,
        position: &DisplaySourceTextPosition,
    ) -> Option<DisplaySourceItem> {
        let item = self.read_source_cursor(
            source,
            context,
            *position,
            BufferTextDisplayReplacementMode::InlineSourceItems,
        )?;
        let (item, source_char) = item;
        let BufferTextCursorItem::Item(item) = item else {
            debug_assert!(false, "inline source cursor surfaced a buffer replacement");
            return None;
        };
        self.align_display_item(*position, source_char, item)
    }

    pub(crate) fn next_source_consumption_item<B: LayoutBufferView + ?Sized>(
        &mut self,
        source: &mut BufferTextSourceCursor<'_, B>,
        context: &mut DisplaySourceContext<'_>,
        position: &mut DisplaySourceTextPosition,
    ) -> Option<BufferSourceConsumedItem> {
        let item = self.read_source_cursor(
            source,
            context,
            *position,
            BufferTextDisplayReplacementMode::TypedReplacementItem,
        )?;
        if let BufferTextCursorItem::DisplayPropertyReplacement(replacement) = item.0 {
            return Some(BufferSourceConsumedItem::DisplayPropertyReplacement(
                replacement,
            ));
        }
        if let BufferTextCursorItem::OverlayStrings(strings) = item.0 {
            return Some(BufferSourceConsumedItem::OverlayStrings(strings));
        }
        let BufferTextCursorItem::Item(display_item) = item.0 else {
            unreachable!("replacement cursor item handled above");
        };
        let item = self.align_display_item(*position, item.1, display_item)?;
        let item = self.consume_aligned_display_item(item, position)?;
        self.prepare_render_source_item(item)
            .map(BufferSourceConsumedItem::Renderable)
    }

    #[cfg(test)]
    pub(crate) fn render_item_from_item(
        &mut self,
        item: DisplayItem,
        position: &mut DisplaySourceTextPosition,
    ) -> Option<DisplaySourceItem> {
        let item = self.align_display_item(*position, None, item)?;
        self.consume_aligned_display_item(item, position)
    }

    #[cfg(test)]
    pub(crate) fn render_item_from_source_item(
        &mut self,
        item: DisplaySourceItem,
        position: &mut DisplaySourceTextPosition,
    ) -> Option<DisplaySourceItem> {
        self.consume_aligned_display_item(item, position)
    }

    fn consume_aligned_display_item(
        &mut self,
        item: DisplaySourceItem,
        position: &mut DisplaySourceTextPosition,
    ) -> Option<DisplaySourceItem> {
        item.consume_for_render(position).ok()
    }
}
