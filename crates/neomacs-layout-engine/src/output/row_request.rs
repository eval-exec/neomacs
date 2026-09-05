//! Typed output row lifecycle requests.

use crate::types::LayoutCharPos0;
use neomacs_display_protocol::frame_glyphs::{CursorStyle, GlyphRowRole};
use neomacs_display_protocol::glyph_matrix::GlyphRow;
use neomacs_display_protocol::types::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputRowBeginRequest {
    pub(crate) row: usize,
    pub(crate) role: GlyphRowRole,
    pub(crate) mode_line: bool,
    /// Buffer position where this row's walk begins. `Some` for every
    /// buffer-text row begun by the display walk; the begin stamps the row's
    /// `start/end_charpos` with it, so a row's bounds are REAL from birth
    /// (GNU display_line takes MATRIX_ROW_START_CHARPOS from the iterator at
    /// row entry) and no "unset (0, 0)" construction state ever exists.
    /// Chrome rows and wholesale row installs pass `None`.
    pub(crate) start_charpos: Option<LayoutCharPos0>,
}

#[derive(Clone, Debug)]
pub(crate) struct OutputCompleteRowInstallRequest {
    row: usize,
    role: GlyphRowRole,
    mode_line: bool,
    glyph_row: GlyphRow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OutputRowMetricsRequest {
    /// Stored row Y, relative to the window matrix origin.
    pixel_y: f32,
    height_px: f32,
    ascent_px: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputCurrentRowDecorationRequest {
    MarkTruncatedLeft,
}

#[derive(Clone, Debug)]
pub(crate) enum OutputRowLifecycleRequest {
    Begin(OutputRowBeginRequest),
    /// Roll back a speculative `Begin` that never acquired glyphs or metrics.
    /// The output grid validates that the target is still an empty begun row,
    /// so a caller cannot erase an already committed row accidentally.
    AbandonEmptyBegin {
        row: usize,
    },
    Complete(OutputCompleteRowInstallRequest),
    Metrics {
        row: usize,
        metrics: OutputRowMetricsRequest,
    },
    Finalize {
        row: usize,
    },
    Cursor {
        row: usize,
        col: u16,
        style: CursorStyle,
    },
    CurrentDecoration(OutputCurrentRowDecorationRequest),
}

pub(crate) trait DisplayCurrentRowMutation {
    type Output;

    fn apply(self, row: &mut GlyphRow) -> Self::Output;
}

pub(crate) trait DisplayWindowRowMutation {
    type Output;

    fn apply(self, row: &mut GlyphRow, matrix_cols: usize) -> Self::Output;
}

pub(crate) trait DisplayWindowRowsMutation {
    fn apply(&mut self, row: &mut GlyphRow, matrix_cols: usize);
}

impl OutputRowBeginRequest {
    pub(crate) fn new(row: usize, role: GlyphRowRole, mode_line: bool) -> Self {
        Self {
            row,
            role,
            mode_line,
            start_charpos: None,
        }
    }

    pub(crate) fn text_at(row: usize, start_charpos: LayoutCharPos0) -> Self {
        Self {
            row,
            role: GlyphRowRole::Text,
            mode_line: false,
            start_charpos: Some(start_charpos),
        }
    }

    pub(crate) fn apply_to_row(self, row: &mut GlyphRow) {
        if let Some(start) = self.start_charpos {
            let charpos = start.get().max(0) as usize;
            row.start_charpos = charpos;
            row.end_charpos = charpos;
        }
        row.role = self.role;
        row.enabled = true;
        row.mode_line = self.mode_line;
    }
}

impl OutputCompleteRowInstallRequest {
    pub(crate) fn new(
        row: usize,
        role: GlyphRowRole,
        mode_line: bool,
        glyph_row: GlyphRow,
    ) -> Self {
        Self {
            row,
            role,
            mode_line,
            glyph_row,
        }
    }

    pub(crate) fn from_window_absolute_row(
        row: usize,
        source: &GlyphRow,
        window_bounds: Rect,
    ) -> Self {
        let mut glyph_row = source.clone();
        OutputRowMetricsRequest::new(
            source.pixel_y - window_bounds.y,
            source.height_px,
            source.ascent_px,
        )
        .apply_to_row(&mut glyph_row);
        Self::new(row, glyph_row.role, glyph_row.mode_line, glyph_row)
    }

    pub(crate) fn row_index(&self) -> usize {
        self.row
    }

    pub(crate) fn begin_request(&self) -> OutputRowBeginRequest {
        OutputRowBeginRequest::new(self.row, self.role, self.mode_line)
    }

    pub(crate) fn into_glyph_row(self) -> GlyphRow {
        self.glyph_row
    }
}

impl OutputRowMetricsRequest {
    pub(crate) fn new(pixel_y: f32, height_px: f32, ascent_px: f32) -> Self {
        Self {
            pixel_y,
            height_px,
            ascent_px,
        }
    }

    pub(crate) fn pixel_y(self) -> f32 {
        self.pixel_y
    }

    pub(crate) fn height_px(self) -> f32 {
        self.height_px.max(0.0)
    }

    pub(crate) fn ascent_px(self) -> f32 {
        self.ascent_px.max(0.0).min(self.height_px())
    }

    pub(crate) fn apply_to_row(self, row: &mut GlyphRow) {
        row.pixel_y = self.pixel_y();
        row.height_px = self.height_px();
        row.ascent_px = self.ascent_px();
    }
}

impl OutputRowLifecycleRequest {
    // Only `OutputBuilder::begin_output_row`, itself `#[cfg(test)]`, opens a row
    // this way; window output opens rows through `begin_text_at`.
    #[cfg(test)]
    pub(crate) fn begin(row: usize, role: GlyphRowRole, mode_line: bool) -> Self {
        Self::Begin(OutputRowBeginRequest::new(row, role, mode_line))
    }

    pub(crate) fn begin_text_at(row: usize, start_charpos: LayoutCharPos0) -> Self {
        Self::Begin(OutputRowBeginRequest::text_at(row, start_charpos))
    }

    pub(crate) fn abandon_empty_begin(row: usize) -> Self {
        Self::AbandonEmptyBegin { row }
    }

    pub(crate) fn complete(
        row: usize,
        role: GlyphRowRole,
        mode_line: bool,
        glyph_row: GlyphRow,
    ) -> Self {
        Self::Complete(OutputCompleteRowInstallRequest::new(
            row, role, mode_line, glyph_row,
        ))
    }

    pub(crate) fn complete_window_absolute_row(
        row: usize,
        source: &GlyphRow,
        window_bounds: Rect,
    ) -> Self {
        Self::Complete(OutputCompleteRowInstallRequest::from_window_absolute_row(
            row,
            source,
            window_bounds,
        ))
    }

    pub(crate) fn metrics(row: usize, pixel_y: f32, height_px: f32, ascent_px: f32) -> Self {
        Self::Metrics {
            row,
            metrics: OutputRowMetricsRequest::new(pixel_y, height_px, ascent_px),
        }
    }

    pub(crate) fn finalize(row: usize) -> Self {
        Self::Finalize { row }
    }

    pub(crate) fn cursor(row: usize, col: u16, style: CursorStyle) -> Self {
        Self::Cursor { row, col, style }
    }

    pub(crate) fn current_decoration(decoration: OutputCurrentRowDecorationRequest) -> Self {
        Self::CurrentDecoration(decoration)
    }
}
