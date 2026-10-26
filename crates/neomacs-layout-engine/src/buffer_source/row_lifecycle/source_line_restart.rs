//! A source restart proof borrowed from the live post-newline producer.

use super::*;
use crate::incremental_layout::edit_sync::{EditSyncRowIndex, EditSyncWindowY};
use std::marker::PhantomData;
use std::rc::Rc;

/// Refusal of the canonical post-newline source proof. Unknown producer
/// dependencies retain the ordinary walk; the producer owns their definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceLineRestartRefusal {
    DisplayStringBoundary,
    NotBufferNewline,
    NonAdjacentAdvance,
    SelectiveDisplay,
    Prefix,
    LineNumbers,
    ActiveBox,
    LineHeight,
    LineSpacing,
    ProducerState,
}

#[derive(Debug)]
pub(super) struct CanonicalSourceLineRestart<'step> {
    position: LayoutCharPos0,
    _producer: PhantomData<&'step ()>,
    _mutator: PhantomData<Rc<()>>,
}

impl<'step> CanonicalSourceLineRestart<'step> {
    pub(super) fn certify<B: LayoutBufferView>(
        request: &BufferSourceLineBreakRenderRequest<'_>,
        producer: &'step BufferSourceWalk<'_, B>,
        position: LayoutCharPos0,
        line_numbers: &LineNumberRenderState,
        box_face: &BoxFaceRowState,
    ) -> Result<Self, SourceLineRestartRefusal> {
        let context = request.context;
        if request.display_string_line_break.is_some() {
            return Err(SourceLineRestartRefusal::DisplayStringBoundary);
        }
        if context.text.get(request.source_char.start_byte_idx()) != Some(&b'\n') {
            return Err(SourceLineRestartRefusal::NotBufferNewline);
        }
        if position.get() != request.source_char.start_charpos() + 1 {
            return Err(SourceLineRestartRefusal::NonAdjacentAdvance);
        }
        if context.selective_display != 0 {
            return Err(SourceLineRestartRefusal::SelectiveDisplay);
        }
        if context.has_prefix {
            return Err(SourceLineRestartRefusal::Prefix);
        }
        if line_numbers.is_enabled() {
            return Err(SourceLineRestartRefusal::LineNumbers);
        }
        if box_face.is_active() {
            return Err(SourceLineRestartRefusal::ActiveBox);
        }
        if request.line_height != crate::display_item::DisplayLineHeightPolicy::Default {
            return Err(SourceLineRestartRefusal::LineHeight);
        }
        if request.line_spacing != crate::display_item::DisplayLineSpacingPolicy::Inherit {
            return Err(SourceLineRestartRefusal::LineSpacing);
        }
        if !producer.can_restart_after_buffer_newline(position.get()) {
            return Err(SourceLineRestartRefusal::ProducerState);
        }
        Ok(Self {
            position,
            _producer: PhantomData,
            _mutator: PhantomData,
        })
    }

    pub(super) fn source_position(&self) -> LayoutCharPos0 {
        self.position
    }
}

/// A live source restart coupled to the actual begun row's placement.
/// Its private constructor consumes the canonical producer-borrowed proof;
/// neither a numeric position alone nor a retained row can construct it.
#[derive(Debug)]
pub(crate) struct SourceLineRestart<'step> {
    source: CanonicalSourceLineRestart<'step>,
    row: EditSyncRowIndex,
    window_y: EditSyncWindowY,
}

impl<'step> SourceLineRestart<'step> {
    pub(super) fn new(
        source: CanonicalSourceLineRestart<'step>,
        row: EditSyncRowIndex,
        window_y: EditSyncWindowY,
    ) -> Self {
        Self {
            source,
            row,
            window_y,
        }
    }

    pub(crate) fn source_position(&self) -> LayoutCharPos0 {
        self.source.source_position()
    }

    pub(crate) fn matrix_row_index(&self) -> EditSyncRowIndex {
        self.row
    }

    pub(crate) fn window_y(&self) -> EditSyncWindowY {
        self.window_y
    }
}

static_assertions::assert_not_impl_any!(SourceLineRestart<'static>: Send, Sync, Clone, Copy);
