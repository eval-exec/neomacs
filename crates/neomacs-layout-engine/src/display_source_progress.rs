use crate::display_row::builder::{DisplayPhysicalLineTabState, DisplayRowPosition};
use crate::display_source::DisplaySourceTextPosition;
use neomacs_display_protocol::types::Px;
use neovm_core::buffer::DisplayColumn;

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
    dpvec_index: Option<&'emit mut usize>,
    row: DisplaySourceRowProgressState<'emit>,
    physical_line_tabs: Option<&'emit mut DisplayPhysicalLineTabState>,
    row_text_start_x: Option<&'emit mut Px>,
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
            dpvec_index: None,
            row: DisplaySourceRowProgressState::new(x, col),
            physical_line_tabs: None,
            row_text_start_x: None,
        }
    }

    pub(crate) fn with_dpvec_index(mut self, dpvec_index: &'emit mut usize) -> Self {
        self.dpvec_index = Some(dpvec_index);
        self
    }

    pub(crate) fn with_physical_line_tabs(
        mut self,
        physical_line_tabs: &'emit mut DisplayPhysicalLineTabState,
    ) -> Self {
        self.physical_line_tabs = Some(physical_line_tabs);
        self
    }

    pub(crate) fn with_row_text_start(mut self, x: &'emit mut Px) -> Self {
        self.row_text_start_x = Some(x);
        self
    }

    pub(crate) fn dpvec_index(&self) -> usize {
        self.dpvec_index.as_deref().copied().unwrap_or(0)
    }

    pub(crate) fn set_dpvec_index(&mut self, dpvec_index: usize) {
        if let Some(slot) = self.dpvec_index.as_deref_mut() {
            *slot = dpvec_index;
        }
    }

    pub(crate) fn visual_row_progress(&mut self) -> DisplaySourceVisualRowProgress<'_> {
        DisplaySourceVisualRowProgress {
            row: self.row.reborrow(),
            physical_line_tabs: self.physical_line_tabs.as_deref_mut(),
            row_text_start_x: self.row_text_start_x.as_deref_mut(),
        }
    }

    pub(crate) fn record_row_text_start(&mut self) {
        if let Some(start) = self.row_text_start_x.as_deref_mut() {
            *start = Px(self.row.x());
        }
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
        if *self.charpos != charpos {
            self.set_dpvec_index(0);
        }
        *self.charpos = charpos;
    }

    pub(crate) fn advance_charpos_by_one(&mut self) {
        *self.charpos += 1;
        self.set_dpvec_index(0);
    }

    pub(crate) fn max_charpos(&mut self, charpos: i64) {
        if charpos > *self.charpos {
            self.set_dpvec_index(0);
        }
        *self.charpos = (*self.charpos).max(charpos);
    }

    pub(crate) fn byte_idx(&self) -> usize {
        *self.byte_idx
    }

    pub(crate) fn set_byte_idx(&mut self, byte_idx: usize) {
        if *self.byte_idx != byte_idx {
            self.set_dpvec_index(0);
        }
        *self.byte_idx = byte_idx;
    }

    pub(crate) fn source_position(&self) -> DisplaySourceTextPosition {
        DisplaySourceTextPosition::new(*self.byte_idx, *self.charpos)
            .with_dpvec_index(self.dpvec_index())
    }

    pub(crate) fn apply_row_position(&mut self, position: DisplayRowPosition) {
        self.row.apply_position(position);
    }

    pub(crate) fn apply_source_position(&mut self, position: DisplaySourceTextPosition) {
        *self.byte_idx = position.byte_idx();
        *self.charpos = position.charpos();
        self.set_dpvec_index(position.dpvec_index());
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
            dpvec_index: self.dpvec_index.as_deref_mut(),
            row: self.row.reborrow(),
            physical_line_tabs: self.physical_line_tabs.as_deref_mut(),
            row_text_start_x: self.row_text_start_x.as_deref_mut(),
        }
    }
}

/// Whether pushed source rendering advanced beyond the decorated text origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplaySourceContent {
    Empty,
    Present,
}

static_assertions::assert_impl_all!(DisplaySourceContent: Copy, Send, Sync);
const _: () = assert!(std::mem::size_of::<DisplaySourceContent>() == 1);

/// The visual pen borrowed by a pushed source. Buffer source coordinates stay
/// with the outer iterator; TAB progress and the decorated row's text origin
/// stay shared, so an overlay cannot leave either describing the previous row.
pub(crate) struct DisplaySourceVisualRowProgress<'a> {
    row: DisplaySourceRowProgressState<'a>,
    physical_line_tabs: Option<&'a mut DisplayPhysicalLineTabState>,
    row_text_start_x: Option<&'a mut Px>,
}

impl<'a> DisplaySourceVisualRowProgress<'a> {
    pub(crate) fn new(row: DisplaySourceRowProgressState<'a>) -> Self {
        Self {
            row,
            physical_line_tabs: None,
            row_text_start_x: None,
        }
    }

    pub(crate) fn with_physical_line_tabs(
        mut self,
        tabs: &'a mut DisplayPhysicalLineTabState,
    ) -> Self {
        self.physical_line_tabs = Some(tabs);
        self
    }

    pub(crate) fn with_row_text_start(mut self, x: &'a mut Px) -> Self {
        self.row_text_start_x = Some(x);
        self
    }

    pub(crate) fn reborrow(&mut self) -> DisplaySourceVisualRowProgress<'_> {
        DisplaySourceVisualRowProgress {
            row: self.row.reborrow(),
            physical_line_tabs: self.physical_line_tabs.as_deref_mut(),
            row_text_start_x: self.row_text_start_x.as_deref_mut(),
        }
    }

    pub(crate) fn row_position(&self) -> DisplayRowPosition {
        self.row.row_position().with_tab_coordinates(
            self.physical_line_tabs
                .as_deref()
                .map_or_else(Default::default, |tabs| tabs.coordinates()),
        )
    }

    pub(crate) fn x(&self) -> Px {
        Px(self.row.x())
    }
    pub(crate) fn col(&self) -> DisplayColumn {
        DisplayColumn::new(self.row.col())
    }
    pub(crate) fn set_col(&mut self, col: DisplayColumn) {
        *self.row.col_mut() = col.get();
    }
    pub(crate) fn apply_position(&mut self, position: DisplayRowPosition) {
        self.row.apply_position(position);
    }

    pub(crate) fn source_content(&self, content_x: Px) -> DisplaySourceContent {
        if self.x()
            > self
                .row_text_start_x
                .as_deref()
                .copied()
                .unwrap_or(content_x)
        {
            DisplaySourceContent::Present
        } else {
            DisplaySourceContent::Empty
        }
    }

    pub(crate) fn record_row_text_start(&mut self) {
        if let Some(start) = self.row_text_start_x.as_deref_mut() {
            *start = Px(self.row.x());
        }
    }

    pub(crate) fn continue_after_visual_row(&mut self, content_x: Px) {
        let width = (self.x().get() - content_x.get()).max(0.0);
        if let Some(tabs) = self.physical_line_tabs.as_deref_mut() {
            tabs.continue_after_visual_row(width);
        }
    }

    pub(crate) fn record_wrap_prefix_width(&mut self, width: Px) {
        if let Some(tabs) = self.physical_line_tabs.as_deref_mut() {
            tabs.record_wrap_prefix(width.get());
        }
    }

    pub(crate) fn reset_physical_line_tabs(&mut self) {
        if let Some(tabs) = self.physical_line_tabs.as_deref_mut() {
            tabs.reset_for_physical_line();
        }
    }
}
