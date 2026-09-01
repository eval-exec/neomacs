use crate::display_row::builder::{DisplayPhysicalLineTabState, DisplayRowPosition};
use crate::display_source::DisplaySourceTextPosition;

pub(crate) struct DisplaySourceRowProgressState<'emit> {
    x: &'emit mut f32,
    col: &'emit mut usize,
}

impl<'emit> DisplaySourceRowProgressState<'emit> {
    pub(crate) fn new(x: &'emit mut f32, col: &'emit mut usize) -> Self {
        Self { x, col }
    }

    pub(crate) fn row_position(&self) -> DisplayRowPosition {
        DisplayRowPosition::new(*self.x, *self.col)
    }

    pub(crate) fn x(&self) -> f32 {
        *self.x
    }

    pub(crate) fn col(&self) -> usize {
        *self.col
    }

    pub(crate) fn x_mut(&mut self) -> &mut f32 {
        self.x
    }

    pub(crate) fn col_mut(&mut self) -> &mut usize {
        self.col
    }

    pub(crate) fn coordinates_mut(&mut self) -> (&mut f32, &mut usize) {
        (self.x, self.col)
    }

    pub(crate) fn reborrow(&mut self) -> DisplaySourceRowProgressState<'_> {
        DisplaySourceRowProgressState {
            x: self.x,
            col: self.col,
        }
    }

    pub(crate) fn apply_position(&mut self, position: DisplayRowPosition) {
        *self.x = position.x_px();
        *self.col = position.col();
    }
}

pub(crate) struct DisplaySourceProgressState<'emit> {
    byte_idx: &'emit mut usize,
    charpos: &'emit mut i64,
    row: DisplaySourceRowProgressState<'emit>,
    physical_line_tabs: Option<&'emit mut DisplayPhysicalLineTabState>,
}

impl<'emit> DisplaySourceProgressState<'emit> {
    pub(crate) fn new(
        byte_idx: &'emit mut usize,
        charpos: &'emit mut i64,
        x: &'emit mut f32,
        col: &'emit mut usize,
    ) -> Self {
        Self {
            byte_idx,
            charpos,
            row: DisplaySourceRowProgressState::new(x, col),
            physical_line_tabs: None,
        }
    }

    pub(crate) fn with_physical_line_tabs(
        mut self,
        physical_line_tabs: &'emit mut DisplayPhysicalLineTabState,
    ) -> Self {
        self.physical_line_tabs = Some(physical_line_tabs);
        self
    }

    pub(crate) fn row_position(&self) -> DisplayRowPosition {
        self.row.row_position().with_tab_coordinates(
            self.physical_line_tabs
                .as_deref()
                .map_or_else(Default::default, |state| state.coordinates()),
        )
    }

    pub(crate) fn row_progress(&self) -> &DisplaySourceRowProgressState<'emit> {
        &self.row
    }

    pub(crate) fn row_progress_mut(&mut self) -> &mut DisplaySourceRowProgressState<'emit> {
        &mut self.row
    }

    pub(crate) fn charpos(&self) -> i64 {
        *self.charpos
    }

    pub(crate) fn set_charpos(&mut self, charpos: i64) {
        *self.charpos = charpos;
    }

    pub(crate) fn advance_charpos_by_one(&mut self) {
        *self.charpos += 1;
    }

    pub(crate) fn max_charpos(&mut self, charpos: i64) {
        *self.charpos = (*self.charpos).max(charpos);
    }

    pub(crate) fn byte_idx(&self) -> usize {
        *self.byte_idx
    }

    pub(crate) fn set_byte_idx(&mut self, byte_idx: usize) {
        *self.byte_idx = byte_idx;
    }

    pub(crate) fn source_position(&self) -> DisplaySourceTextPosition {
        DisplaySourceTextPosition::new(*self.byte_idx, *self.charpos)
    }

    pub(crate) fn apply_row_position(&mut self, position: DisplayRowPosition) {
        self.row.apply_position(position);
    }

    pub(crate) fn apply_source_position(&mut self, position: DisplaySourceTextPosition) {
        *self.byte_idx = position.byte_idx();
        *self.charpos = position.charpos();
    }

    pub(crate) fn continue_physical_line_after_visual_row(
        &mut self,
        row_end_x: f32,
        content_x: f32,
    ) {
        if let Some(state) = self.physical_line_tabs.as_deref_mut() {
            state.continue_after_visual_row((row_end_x - content_x).max(0.0));
        }
    }

    pub(crate) fn record_wrap_prefix_width(&mut self, width_px: f32) {
        if let Some(state) = self.physical_line_tabs.as_deref_mut() {
            state.record_wrap_prefix(width_px);
        }
    }

    pub(crate) fn reset_physical_line_tabs(&mut self) {
        if let Some(state) = self.physical_line_tabs.as_deref_mut() {
            state.reset_for_physical_line();
        }
    }

    pub(crate) fn reborrow(&mut self) -> DisplaySourceProgressState<'_> {
        DisplaySourceProgressState {
            byte_idx: self.byte_idx,
            charpos: self.charpos,
            row: self.row.reborrow(),
            physical_line_tabs: self.physical_line_tabs.as_deref_mut(),
        }
    }
}
